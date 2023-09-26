use std::sync::atomic::AtomicBool;

use actix::{Actor, StreamHandler};
use actix_cors::Cors;
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use network_interface::NetworkInterface;
use network_interface::NetworkInterfaceConfig;

static HOOK_STATE: AtomicBool = AtomicBool::new(true);

/// Define HTTP actor
struct MyWs;

impl Actor for MyWs {
    type Context = ws::WebsocketContext<Self>;
}

/// Handler for ws::Message message
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWs {
    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.text(if HOOK_STATE.load(std::sync::atomic::Ordering::SeqCst) {
            "enable hook"
        } else {
            "disable hook"
        });
    }

    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, _ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                println!("{}", text);
            }
            _ => (),
        }
    }
}

async fn index(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    ws::start(MyWs {}, &req, stream)
}

async fn log_js(_req: HttpRequest, _stream: web::Payload) -> Result<HttpResponse, Error> {
    Ok(
        HttpResponse::Ok().content_type("application/json").body(r#"
        const _ = (() => {
            console.log('==> log js loaded')

            const script = document.scripts[document.scripts.length - 1]
            const scriptUrl = new URL(script.src, location.href)
            const url = `${scriptUrl.protocol.replace('http', 'ws')}//${scriptUrl.host}/ws/`
            const ws = window.__log_ws__ = window.__log_ws__ || new WebSocket(url)
            ws.onmessage = event => {
                const handler = {
                    'enable hook': () => {hook()},
                    'disable hook': () => {restore()}
                }[event.data]
                handler()
            }
            window._log = (...args) => {
                setTimeout(() => {
                    ws.send(args.map(s => typeof s === 'object' && s ? JSON.stringify(s) : ('' + s)).join(' '))
                })
            }

            let restore = () => {}
            let hooked = false
            const hook = () => {
                if (hooked) { return }
                console.log('==> enable hook')
                hooked = true
                let originalLog = Object.getOwnPropertyDescriptor(window.console, 'log')
                Object.defineProperty(window.console, 'log', {
                    configurable: true,
                    enumerable: false,
                    get: () => window._log,
                    set: () => {}
                })
                restore = () => {
                    console.log('==> disable hook')
                    hooked = false
                    Object.defineProperty(window.console, 'log', originalLog)
                }
            }
        })()
    "#)
    )
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let port = 9000;
    let server = HttpServer::new(|| {
        let cors = Cors::default()
            .allowed_origin_fn(|_, _| true)
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .max_age(3600);
        App::new()
            .wrap(cors)
            .route("/ws/", web::get().to(index))
            .route("/log.js", web::get().to(log_js))
    })
    .bind(("0.0.0.0", port))?;

    println!("Listening at:");
    let network_interfaces = NetworkInterface::show().unwrap();
    for itf in network_interfaces.iter() {
        for addr in &itf.addr {
            println!("http://{}:{}", addr.ip(), port);
            println!(
                r#"    <script src="http://{}:{}/log.js"></script>"#,
                addr.ip(),
                port
            );
        }
    }

    println!("Usage:");

    std::thread::spawn(|| loop {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        let line = line.trim();
        if line == "h" {
            if HOOK_STATE.load(std::sync::atomic::Ordering::SeqCst) {
                HOOK_STATE.store(false, std::sync::atomic::Ordering::SeqCst);
                println!("Hook disabled");
            } else {
                HOOK_STATE.store(true, std::sync::atomic::Ordering::SeqCst);
                println!("Hook enabled");
            }
        }
    });

    server.run().await
}
