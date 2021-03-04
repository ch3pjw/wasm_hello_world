extern crate cfg_if;

use {
    cfg_if::cfg_if,
    futures::{stream::StreamExt, SinkExt},
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

    spawn_local(async move {
        let (ws, mut rcv_q) = websockets::go("wss://echo.websocket.org").await.expect_throw("oops");
        ws.send_with_str("hello world!");
        loop {
            match rcv_q.next().await {
                None => error!("wat"),
                Some(websockets::WsMsg::Msg(msg)) => info!("woot, msg: {}", msg),
                Some(websockets::WsMsg::Err(())) => error!("I died"),
            }
        }
    });

    yew::App::<Model>::new().mount_to_body();
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

struct Model {}

impl yew::Component for Model {
    type Message = ();
    type Properties = ();

    fn create(_: Self::Properties, _: yew::ComponentLink<Self>) -> Self {
        Self {}
    }

    fn update(&mut self, _: Self::Message) -> yew::ShouldRender {
        false
    }

    fn change(&mut self, _: Self::Properties) -> yew::ShouldRender {
        false
    }

    fn view(&self) -> yew::Html {
        yew::html! {
            <h1>{ "Hello World" }</h1>
        }
    }
}
