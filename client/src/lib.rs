extern crate cfg_if;

use {
    cfg_if::cfg_if,
    futures::{stream::StreamExt, channel::mpsc},
    log::{error, info},
    wasm_bindgen::prelude::*,
    wasm_bindgen_futures::spawn_local,
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
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<String>(32);
        spawn_local(async move {
            loop {
                match cmd_rx.next().await {
                    None => error!("oh noes"),
                    Some(txt) => {
                        info!("Send command received, sending message...");
                        ws.send_with_str(&txt);
                    }
                }
            }
        });

        let ui = yew::App::<UiModel>::new().mount_to_body_with_props(UiProps{ cmd_tx });
        spawn_local(async move {
            loop {
                match msg_rx.next().await {
                    None => error!("wat"),
                    Some(websockets::WsMsg::Msg(msg)) => ui.send_message(UiMsg::ReceivedMsg(msg)),
                    Some(websockets::WsMsg::Err(())) => error!("I died"),
                }
            }

        });

    });
    info!("hello again");
}

struct UiModel {
    props: UiProps,
    state: UiState,
}

#[derive(Clone, yew::Properties)]
struct UiProps {
    cmd_tx: mpsc::Sender<String>,
}

struct UiState {
    received_count: u32
}

enum UiMsg {
    ReceivedMsg(String),
}

impl yew::Component for UiModel {
    type Message = UiMsg;
    type Properties = UiProps;

    fn create(props: Self::Properties, _: yew::ComponentLink<Self>) -> Self {
        Self { props, state: UiState{ received_count: 0 } }
    }

    fn update(&mut self, msg: Self::Message) -> yew::ShouldRender {
        match msg {
            UiMsg::ReceivedMsg(msg) => {
                info!("UI received a message! {}", msg);
                self.state.received_count += 1;
                true
            }
        }
    }

    fn change(&mut self, _: Self::Properties) -> yew::ShouldRender {
        false
    }

    fn view(&self) -> yew::Html {
        yew::html! {
            <div>
              <h1>{ "Hello World: " }<Counter n=self.state.received_count/></h1>
              <Transmitter default_msg="Hello World!" cmd_tx=self.props.cmd_tx.clone()/>
            </div>
        }
    }
}

#[repr(transparent)]
#[derive(Clone, PartialEq, yew::Properties)]
struct U32Prop { n: u32 }

#[repr(transparent)]
struct Counter { props: U32Prop }

impl yew::Component for Counter {
    type Message = ();
    type Properties = U32Prop;

    fn create(props: Self::Properties, _: yew::ComponentLink<Self>) -> Self {
        Counter { props }
    }

    fn update(&mut self, _: Self::Message) -> yew::ShouldRender {
        false
    }

    fn change(&mut self, props: Self::Properties) -> yew::ShouldRender {
        if props != self.props {
            self.props = props;
            true
        } else {
            false
        }
    }

    fn view(&self) -> yew::Html {
        yew::html! {
            { self.props.n }
        }
    }
}


struct Transmitter {
    props: TransmitterProps,
    link: yew::ComponentLink<Self>,
    current_msg: Option<String>,
}

#[derive(Clone, yew::Properties)]
struct TransmitterProps {
    default_msg: String,
    cmd_tx: mpsc::Sender<String>,
}

#[derive(Debug)]
enum TransmitterMsg {
    SendMsg,
    Input(String),
}

impl Transmitter {
    fn msg(&self, current_msg: &Option<String>) -> String {
        current_msg.as_ref().unwrap_or(&self.props.default_msg).clone()
    }
}

impl yew::Component for Transmitter {
    type Message = TransmitterMsg;
    type Properties = TransmitterProps;

    fn create(props: Self::Properties, link: yew::ComponentLink<Self>) -> Self {
        Transmitter { props, link, current_msg: None }
    }

    fn change(&mut self, props: Self::Properties) -> yew::ShouldRender {
        self.props.default_msg = props.default_msg;
        false
    }

    fn update(&mut self, msg: Self::Message) -> yew::ShouldRender {
        match msg {
            TransmitterMsg::Input(txt) => {
                self.current_msg = if txt == "" { None } else { Some(txt) };
            },
            TransmitterMsg::SendMsg => {
                let current_msg = self.current_msg.take();
                self.props.cmd_tx.try_send(self.msg(&current_msg));
            }
        }
        true
    }

    fn view(&self) -> yew::Html {
        yew::html! {
            <div>
                <input
                    value=self.current_msg.as_ref().unwrap_or(&"".to_string())
                    oninput=self.link.callback(|evt: yew::InputData| TransmitterMsg::Input(evt.value))
                    onkeypress=self.link.batch_callback(| evt: yew::events::KeyboardEvent| {
                        if evt.key() == "Enter" { vec![TransmitterMsg::SendMsg] } else { vec![] }
                    })
                />
                <button onclick=self.link.callback(|_| TransmitterMsg::SendMsg )>
                    { format!("Send {:?}", self.msg(&self.current_msg)) }
                </button>
            </div>
        }
    }
}
