# smuggle-host

[![Rust](https://github.com/fpco/smuggle-host/actions/workflows/rust.yml/badge.svg)](https://github.com/fpco/smuggle-host/actions/workflows/rust.yml)

**NOTE** This repository is highly experimental, caveat emptor!

## The problem

Suppose you're writing a reverse proxy within an Istio-enabled Kubernetes cluster. One issue [you'll run into](https://www.fpcomplete.com/blog/istio-mtls-debugging-story/) is that, if you set the `Host` header to the originally requested host, the Envoy proxy will try to connect to that host, causing things to break. The simplest solution to this problem is to strip off the `Host` header for outgoing reverse proxy requests.

This works fine, until you run into a tool that needs to know the originating host header. In that case, there's no way to tell Envoy to please pass along the original header.

## Smuggling

That's where this tool comes into play. It works as a simple reverse proxy inside the destination pod. Instead of sending requests directly from Envoy to the real application, `smuggle-host` sits in between and rewrites some HTTP header to `Host`, thereby smuggling it past Envoy.

For example, if your application is running inside the pod on port 5000, and your external reverse proxy is rewriting `Host` headers to `Smuggled-Host` and connecting to port 4000, you can run `smuggle-host` with:

```shellsession
$ smuggle-host --bind 0.0.0.0:4000 --desthost 127.0.0.1 --destport 5000 --smuggle-header X-Smuggled-Host
# Using default values
$ smuggle-host --bind 0.0.0.0:4000 --destport 5000
```
