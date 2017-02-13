#[macro_use]
extern crate serde_derive;

extern crate serde_json;
extern crate ws;

use std::thread;
use std::sync::mpsc;

use ws::listen;

// This might be what i need:
// https://github.com/modrzew/hnfd/blob/7826be05cfe0c83aec43ceeb50d3362655b9d800/src/server.rs

// It's name "kind" instead of "type" as "type" is a reserved word
#[derive(Deserialize, Debug)]
enum MessageKind {
    #[serde(rename = "registerClient")]
    Register,
}

/// The deserialized message received as JSON via the WebSocket
#[derive(Deserialize, Debug)]
struct RilMessage {
    // The client ID comes from the  `RadioInterfaceLayer()` in
    // `dom/system/gonk/RadioInterfaceLayer.js` and is an unsigned integer. There's a client
    // for every radio interface.
    // NOTE vmx 2017-02-12: I don't expect more than 256 radio interfaces on one device
    #[serde(rename = "rilMessageClientId")]
    client_id: u8,
    #[serde(rename = "rilMessageToken")]
    token: u64,
    #[serde(rename = "rilMessageType")]
    kind: MessageKind,
}

#[derive(Debug)]
struct RegisterMessage {
    ril_message: RilMessage,
    websocket_sender: ws::Sender,
}

#[derive(Debug)]
enum Message {
    // Registering a client is a special case, we can't just forward the message as the payload
    // also needs to contain the WebSocket it is using
    Register(RegisterMessage),
    // The messages we are receiving are just forwarded to the JS Worker
    RilMessage(RilMessage),
}


struct Client {
    websocket_sender: ws::Sender,
}


struct JsWorker {
    // Each client has an ID that come from  `RadioInterfaceLayer()` in
    // `dom/system/gonk/RadioInterfaceLayer.js` and is an unsigned integer. There's a client
    // for every radio interface. That client ID corresponds to the index position within
    // this vector
    clients: Vec<Option<Client>>,
    message_receiver: mpsc::Receiver<Message>,
}

impl JsWorker {
    fn run(&mut self) {
        println!("JS Worker got started!");
        println!("JS Worker is waiting for a message!");

        //XXX vmx 2017-02-08: GO ON HERE and send the websocket along, so that we can send
        //    messages async. It would make sense to have a loop below that is waiting for
        //    different kind of messages. One message will be sent by the websocket on
        //    the connection start (which will include the websocket itself). Others will
        //    be sent by postMessage() to actually send something back over the socket

        while let Ok(message) = self.message_receiver.recv() {
            println!("JS Worker received a message: {:?}", message);
            match message {
                Message::Register(RegisterMessage{ril_message, websocket_sender}) => {
                    self.handle_register(ril_message.client_id, websocket_sender);
                },
                Message::RilMessage(_) => {
                    println!("JS Worker received a ril message");
                },
            }
        }
    }

    /// Register a new client
    fn handle_register(&mut self, client_id: u8, websocket_sender: ws::Sender) {
        println!("JS Worker handling register");
        self.ensure_clients_length(client_id);
        self.clients[client_id as usize] = Some(Client{
            websocket_sender: websocket_sender,
        });
    }

    /// Make sure the vector has enough items to address it directly with an index
    ///
    /// This corresponds to `EnsureLengthAtLeast()` from Mozilla's `nsTArray`.
    fn ensure_clients_length(&mut self, client_id: u8) {
        for _ in self.clients.len()..client_id as usize + 1 {
            self.clients.push(None);
        }
    }
}


struct WebsocketServer {
    websocket_sender: ws::Sender,
    // The sender side of the Rust channel between the server and the JS Worker
    message_sender: mpsc::Sender<Message>,
}

impl ws::Handler for WebsocketServer {
    fn on_open(&mut self, _: ws::Handshake) -> ws::Result<()> {
        println!("new WebSocket connection");
        Ok(())
    }

    fn on_message(&mut self, websocket_message: ws::Message) -> ws::Result<()> {
        println!("received a message: {:?}", websocket_message);

        if let ws::Message::Text(json) = websocket_message {
            let ril_message: RilMessage = serde_json::from_str(&json).unwrap();
            println!("received a message JSON parssed: {:?}", ril_message);
            let message = match ril_message.kind {
                MessageKind::Register => Message::Register(RegisterMessage{
                    ril_message: ril_message,
                    websocket_sender: self.websocket_sender.clone(),
                }),
                _ => Message::RilMessage(ril_message),
            };
            self.message_sender.send(message);
        }
        Ok(())
    }

    fn on_close(&mut self, code: ws::CloseCode, reason: &str) {
        match code {
            ws::CloseCode::Normal => println!("The client is done with the connection."),
            ws::CloseCode::Away   => println!("The client is leaving the site."),
            ws::CloseCode::Abnormal => println!(
                "Closing handshake failed! Unable to obtain closing status from client."),
            _ => println!("The client encountered an error: {}", reason),
        }
    }

    fn on_error(&mut self, error: ws::Error) {
        println!("The server encountered an error: {:?}", error);
    }

}


fn main() {
    let (sender, receiver): (mpsc::Sender<Message>, mpsc::Receiver<Message>) = mpsc::channel();

    let mut js_worker = JsWorker {
        clients: Vec::new(),
        message_receiver: receiver,
    };

    thread::spawn(move || {
        js_worker.run();
    });

    listen("127.0.0.1:3012", |websocket_sender| {
        WebsocketServer {
            websocket_sender: websocket_sender,
            message_sender: sender.clone(),
        }
    }).unwrap()
}
