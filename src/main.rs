use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use anyhow::*;
use clap::Clap;
use hyper::{
    header::{HeaderName, HOST},
    http::uri::{Authority, Parts, Scheme},
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Client, Request, Response, Server, StatusCode, Uri,
};

/// Command line options
#[derive(Clap, Debug)]
struct Opt {
    /// Turn on verbose output. Can be overridden by RUST_LOG env var
    #[clap(short, long)]
    verbose: bool,
    /// Host/port to bind to
    #[clap(long, default_value = "0.0.0.0:3000")]
    bind: String,
    /// Host to direct requests to
    #[clap(long, default_value = "127.0.0.1")]
    desthost: String,
    /// Port to direct requests to
    #[clap(long)]
    destport: u16,
    /// HTTP request header containing the new Host header
    #[clap(long, default_value = "X-Smuggle-Host")]
    smuggle_header: HeaderName,
}

impl Opt {
    /// Initialize the logger based on command line settings
    fn init_logger(&self) {
        use env_logger::{Builder, Target};
        use log::LevelFilter::*;
        let mut builder = Builder::from_default_env();
        let level = if self.verbose { Debug } else { Info };
        builder.filter_module(env!("CARGO_CRATE_NAME"), level);
        builder.target(Target::Stderr).init();
    }
}

/// State of the application
struct App {
    /// Outgoing HTTP(S) connections
    client: Client<hyper::client::HttpConnector>,
    /// HTTP request header containing the new Host header
    smuggle_header: HeaderName,
    /// Destination
    authority: Authority,
}

impl App {
    fn new(opt: Opt) -> Result<Self> {
        let client = Client::new();
        let authority = format!("{}:{}", opt.desthost, opt.destport)
            .parse()
            .context("Unable to parse Authority")?;
        Ok(App {
            client,
            smuggle_header: opt.smuggle_header,
            authority,
        })
    }

    async fn handle_request(
        self: Arc<Self>,
        uuid: uuid::Uuid,
        conn: SocketAddr,
        mut req: Request<Body>,
    ) -> Result<Response<Body>> {
        log::debug!("{}: Incoming request from {}: {:?}", uuid, conn, req);
        for header in HOP_BY_HOPS {
            req.headers_mut().remove(*header);
        }

        let host = req
            .headers_mut()
            .remove(&self.smuggle_header)
            .with_context(|| {
                format!(
                    "Received incoming request without smuggle header {:?}",
                    self.smuggle_header
                )
            })?;
        req.headers_mut().insert(HOST, host);

        let mut parts = Parts::default();
        parts.scheme = Some(Scheme::HTTP);
        parts.authority = Some(self.authority.clone());
        parts.path_and_query = req.uri_mut().path_and_query().cloned();
        *req.uri_mut() = Uri::from_parts(parts).context("Unable to construct destination URI")?;
        self.client
            .request(req)
            .await
            .context("Error performing reverse proxied request")
    }
}

/// Hop by hop headers that should not be forwarded
///
/// See https://www.freesoft.org/CIE/RFC/2068/143.htm
const HOP_BY_HOPS: &[&str] = &[
    "Connection",
    "Keep-alive",
    "Public",
    "Proxy-Authenticate",
    "Transfer-Encoding",
    "Upgrade",
];

#[tokio::main]
async fn main() -> Result<()> {
    let opt = Opt::parse();
    opt.init_logger();
    log::debug!("opt: {:?}", opt);

    let addr: SocketAddr = opt.bind.parse().context("Cannot parse as bind host/port")?;
    let app = Arc::new(App::new(opt)?);

    let make_svc = make_service_fn(move |conn: &AddrStream| {
        let app = app.clone();
        let conn = conn.remote_addr();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let app = app.clone();
                async move {
                    let uuid = uuid::Uuid::new_v4();
                    let res = app
                        .clone()
                        .handle_request(uuid, conn, req)
                        .await
                        .unwrap_or_else(|err| {
                            log::error!("Unhandled error occurred. uuid=={}: {:?}", uuid, err);
                            let mut res = Response::new(
                                format!("An unhandled error occurred, error identifier {}", uuid)
                                    .into(),
                            );
                            *res.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            res
                        });
                    Ok::<_, Infallible>(res)
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    server
        .await
        .context("Hyper server exited, which should not happen")
}
