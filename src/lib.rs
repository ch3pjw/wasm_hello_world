extern crate cfg_if;
mod utils;

use {
    wasm_bindgen::prelude::*,
    wasm_bindgen_futures::spawn_local,

    cfg_if::cfg_if,
    futures::{stream::StreamExt},
    log::{error, info},
    pharos::*,
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

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern {
    fn alert(s: &str);
}

#[wasm_bindgen]
pub fn main() {
    init_log();
    spawn_local(async {
        let (mut ws, _wsio) = WsMeta::connect("wss://echo.websocket.org", None).await.expect_throw( "failed :-(");
        let mut evts = ws.observe(ObserveConfig::default()).await.expect_throw("observe died");
        ws.close().await;

        while let Some(evt) = evts.next().await {
            info!("Received event: {:?}", evt);
        }
        info!("Hello, wasm-hello-world! I closed a websocket");
    });
}
