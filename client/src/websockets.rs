use {
    futures::{
        channel::mpsc,
        stream::StreamExt,
    },
    js_sys,
    log::{error, info},
    std::fmt::Debug,
    thiserror::Error,
    wasm_bindgen::{convert::FromWasmAbi, prelude::*, JsCast},
    web_sys::{ErrorEvent, MessageEvent, WebSocket},
};

pub async fn go<'a>(url: &'a str) -> Result<(WebSocket, mpsc::Receiver<WsMsg>), WsError<'a>> {
    let (rcv_tx, rcv_rx) = mpsc::channel(32);
    let ws = WebSocket::new_with_str(url, "protocols-balh").map_err(
        |e| WsError::ConnectionFailed{ url, err: e }
    )?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    let mut tx = rcv_tx.clone();
    set_callback(
        |cb| ws.set_onmessage(cb),
        move |e: MessageEvent| {
            info!("onmessage: {:?} {:?}", e, e.data());
            if let Ok(msg) = e.data().dyn_into::<js_sys::JsString>() {
                send_mpsc(&mut tx, WsMsg::Msg(msg.into()))
            } else {
                error!("error unpacking message!")
            }
        }
    );

    // FIXME: The JS WebSockets API appears to send connection errors to this error handler, so we
    // should set a handler like the onopen one, then switch to this one once our connection is
    // established:
    let mut tx = rcv_tx.clone();
    set_callback(
        |cb| ws.set_onerror(cb),
        move |e: ErrorEvent| {
            error!("onerror: {:?}", e);
            send_mpsc(&mut tx, WsMsg::Err(()))
        }
    );

    // TODO: This was supposed to be a futures::channel::oneshot, but it didn't type check with
    // onshot::Sender.send() in the closure for unknown reasons:
    let (mut connected_tx, mut connected_rx) = mpsc::channel(1);
    set_callback(
        |cb| ws.set_onopen(cb),
        // FIXME: `e` is blatently not a MessageEvent and will die horribly if I try to look at it
        // as such, I expect:
        move |e: JsValue| {
            // FIXME: debug assert the ready state here
            info!("I have no idea what this event is... {:?}", e);
            send_mpsc(&mut connected_tx, ())
        }
    );
    connected_rx.next().await;
    connected_rx.close();
    Ok((ws, rcv_rx))
}

#[derive(Clone)]
pub enum WsMsg {
    Msg(String),
    Err(()),
}

#[derive(Debug, Error)]
pub enum WsError<'a> {
    #[error("Failed to {url} connect: {err:?}")]
    ConnectionFailed{ url: &'a str, err: JsValue },
}

// Wraps up a lot of boilerplate closure wrappy stuff by adding even more confusing types! However,
// it keeps all the "stuff to remember" in one place and should hopefully make the client code
// easier to read.
fn set_callback<Evt: 'static, S, CB: 'static>(setter: S, cb: CB)
where
    Evt: FromWasmAbi,
    S: Fn(Option<&js_sys::Function>), // TODO: nicer way to pass WebSocket method?
    CB: FnMut(Evt),
{
    let cb = Closure::wrap(Box::new(cb) as Box<dyn FnMut(Evt)>);
    setter(Some(cb.as_ref().unchecked_ref()));
    // Keep the callback alive:
    cb.forget()
}

fn send_mpsc<T>(sender: &mut mpsc::Sender<T>, value: T) {
    log_err((), sender.try_send(value))
}

fn log_err<T, E: Debug>(def: T, r: Result<T, E>) -> T {
    match r {
        Ok(v) => v,
        Err(err) => { error!("{:?}", err); def }
    }
}
