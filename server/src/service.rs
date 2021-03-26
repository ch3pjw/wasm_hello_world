use {
    futures::{
        future::{ok, ready, Ready},
        channel::mpsc,
    },
    hyper::{
        Body, Request, Response,
        service::Service,
    },
    std::{
        convert::Infallible,
        task::{Context, Poll},
    },
};

pub struct ConnectionHandler<'a, Msg, F> {
    tx: mpsc::UnboundedSender<Msg>,
    f: &'a F
}

impl<'a, Msg, E, F: Fn(Request<Body>, mpsc::UnboundedSender<Msg>) -> Result<Response<Body>, E>> ConnectionHandler<'a, Msg, F> {
    pub fn new(f: &'a F) -> (Self, mpsc::UnboundedReceiver<Msg>) {
    let (tx, rx) = mpsc::unbounded();
    (ConnectionHandler { tx, f }, rx)
    }
}

impl<'a, Conn, Msg, F> Service<Conn> for ConnectionHandler<'a, Msg, F> {
    type Response = RequestHandler<'a, Msg, F>;
    type Error = Infallible;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: Conn) -> Self::Future {
        ok(RequestHandler { tx: self.tx.clone(), f: self.f })
    }
}

pub struct RequestHandler<'a, Msg, F> {
    tx: mpsc::UnboundedSender<Msg>,
    f: &'a F
}

impl<Msg, E, F: Fn(Request<Body>, mpsc::UnboundedSender<Msg>) -> Result<Response<Body>, E>>
Service<Request<Body>> for RequestHandler<'_, Msg, F> {
    type Response = Response<Body>;
    type Error = E;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        ready((self.f)(req, self.tx.clone()))
    }
}
