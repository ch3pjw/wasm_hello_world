mod utils;

use {
    wasm_bindgen::prelude::*,
    wasm_bindgen_futures::spawn_local,

    futures::{stream::StreamExt},
    pharos::*,
    ws_stream_wasm::*,
};

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
pub fn greet() {
    let program = async {
        let (mut ws, _wsio) = WsMeta::connect("wss://echo.websocket.org", None).await.expect_throw( "failed :-(");
        let mut evts = ws.observe(ObserveConfig::default()).await.expect_throw("observe died");
        ws.close().await;
        assert!(evts.next().await.unwrap_throw().is_closing());
        assert!(evts.next().await.unwrap_throw().is_closed());
        alert("Hello, wasm-hello-world! I closed a websocket");
    };
    spawn_local(program);
}
