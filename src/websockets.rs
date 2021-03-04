use {
    futures::{
        channel::mpsc,
        Future,
    },
    js_sys,
    log::{error, info},
    wasm_bindgen::{convert::FromWasmAbi, prelude::*, JsCast},
    web_sys::{ErrorEvent, MessageEvent, WebSocket},
};

pub fn go() -> Result<(WebSocket, mpsc::Receiver<WsMsg>), JsValue> {
    let (mut rcv_tx, mut rcv_rx) = mpsc::channel(32);
    let ws = WebSocket::new("wss://echo.websocket.org")?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    let mut r = rcv_tx.clone();
    set_callback(
        |cb| ws.set_onmessage(cb),
        move |e: MessageEvent| send_mpsc(&mut r, WsMsg::Msg(())),
    );

    let mut r = rcv_tx.clone();
    set_callback(
        |cb| ws.set_onerror(cb),
        move |e: ErrorEvent| send_mpsc(&mut r, WsMsg::Err(())),
    );

    set_callback(
        |cb| ws.set_onopen(cb),
        move |e: MessageEvent| {
            info!("I have no idea what this event is... {:?}", e);
            send_mpsc(&mut rcv_tx, WsMsg::Msg(()))
        },
    );
    Ok((ws, rcv_rx))
}

#[derive(Clone, Copy)]
pub enum WsMsg {
    Msg(()),
    Err(()),
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
    match sender.try_send(value) {
        Ok(()) => (),
        Err(err) => error!("{:?}", err),
    }
}
