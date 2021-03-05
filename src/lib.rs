extern crate cfg_if;

use {
    cfg_if::cfg_if,
    futures::{stream::StreamExt, SinkExt, channel::mpsc},
    log::{error, info},
    pharos::*,
    wasm_bindgen::prelude::*,
    wasm_bindgen_futures::spawn_local,
    ws_stream_wasm::*,
};

cfg_if! {
    if #[cfg(feature = "console_log")] {
        fn init_log() {
            use {console_log, log::Level};
            console_log::init_with_level(Level::Trace).expect(
                "error initialising log"
            );
        }
    } else {
        fn init_log() {}
    }
}

mod utils;
mod websockets;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
}

#[wasm_bindgen(start)]
pub fn maine() {
    utils::set_panic_hook();
    init_log();
    yew::initialize();

    spawn_local(async {
        let (ws, mut msg_rx) = websockets::go("wss://echo.websocket.org").await.expect_throw("oops");
        spawn_local(async move {
            loop {
                match msg_rx.next().await {
                    None => error!("wat"),
                    Some(websockets::WsMsg::Msg(msg)) => info!("woot, msg: {}", msg),
                    Some(websockets::WsMsg::Err(())) => error!("I died"),
                }
            }

        });

        let (cmd_tx, mut cmd_rx) = mpsc::channel(32);
        spawn_local(async move {
            loop {
                match cmd_rx.next().await {
                    None => error!("oh noes"),
                    Some(()) => {
                        info!("Send command received, sending message...");
                        ws.send_with_str("hello world!");
                    }
                }
            }
        });

        yew::App::<UiModel>::new().mount_to_body_with_props(UiProps{ cmd_tx });
    });
    // spawn_local(async {
        // let (mut ws, mut stream) = match WsMeta::connect("wss://echo.websocket.org", None).await {
            // Ok(x) => x,
            // Err(ws_err) => {
                // error!("Error opening WebSocket {:?}", ws_err);
                // return;
            // }
        // };

        // let mut events = ws
            // .observe(ObserveConfig::default())
            // .await
            // .expect_throw("observe died");
        // spawn_local(async move {
            // loop {
                // match events.next().await {
                    // None => {
                        // error!("WebSocket closed unexpectedly!");
                        // break;
                    // }
                    // Some(WsEvent::Closed(close_event)) => {
                        // if close_event.was_clean {
                            // info!("WebSocket closed cleanly");
                        // } else {
                            // error!("WebSocket closed uncleanly: {}", close_event.reason);
                        // }
                        // break;
                    // }
                    // Some(WsEvent::WsErr(ws_err)) => error!("Received error: {:?}", ws_err),
                    // Some(event) => info!("Received event: {:?}", event),
                // }
            // }
        // });

        // stream
            // .send(WsMessage::Text("Hello World!".to_string()))
            // .await
            // .expect_throw("sending failed");
        // info!("blah {:?}", stream.next().await);

        // match ws.close().await {
            // Ok(close_event) => info!("Logging closed here too {:?}", close_event),
            // Err(ws_err) => error!("Got an error: {:?}", ws_err),
        // }
        // info!("Hello, wasm-hello-world! I closed a websocket");
    // });
    info!("hello again");
}

struct UiModel {
    props: UiProps,
    state: UiState,
    link: yew::ComponentLink<Self>
}

#[derive(Clone, yew::Properties)]
struct UiProps {
    cmd_tx: mpsc::Sender<()>
}

struct UiState {}

enum UiMsg {
    SendHello
}

impl yew::Component for UiModel {
    type Message = UiMsg;
    type Properties = UiProps;

    fn create(props: Self::Properties, link: yew::ComponentLink<Self>) -> Self {
        Self { props, state: UiState{}, link }
    }

    fn update(&mut self, msg: Self::Message) -> yew::ShouldRender {
        match msg {
            SendHello => {
                info!("SendHello UI event received, issuing send command...");
                self.props.cmd_tx.try_send(());
            }
        }
        false
    }

    fn change(&mut self, _: Self::Properties) -> yew::ShouldRender {
        false
    }

    fn view(&self) -> yew::Html {
        yew::html! {
            <div>
              <h1>{ "Hello World" }</h1>
              <button onclick=self.link.callback(|_| UiMsg::SendHello)>{ "Send Hello World!" }</button>
            </div>
        }
    }
}
