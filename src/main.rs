#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate ws;

use std::cell::RefCell;
use std::fs::File;
use std::thread;
use std::sync::mpsc;

use ws::listen;



#[macro_use] extern crate js;

use js::conversions::{FromJSValConvertible, ToJSValConvertible};
use js::jsapi::AutoCheckCannotGC;
use js::jsapi::AutoAssertOnGC;
use js::jsapi::JS_AtomizeString;
use js::jsapi::JS_AtomizeAndPinString;
use js::jsapi::CallArgs;
use js::jsapi::CompartmentOptions;
use js::jsapi::HandleValueArray;
use js::jsapi::HandleObject;
use js::jsapi::NullHandleValue;
use js::jsapi::JSAutoCompartment;
use js::jsapi::JSContext;
use js::jsapi::JS_CallFunctionName;
use js::jsapi::JS_DefineFunction;
use js::jsapi::JS_EncodeStringToUTF8;
use js::jsapi::JS_GetArrayBufferViewData;
use js::jsapi::JS_GetArrayBufferViewType;
use js::jsapi::JS_GetTypedArrayByteLength;
use js::jsapi::JS_IsTypedArrayObject;
use js::jsapi::JS_NewGlobalObject;
use js::jsapi::JS_ParseJSON;
use js::jsapi::JS_ParseJSON1;
use js::jsapi::JS_ReportError;
use js::jsapi::JS_Stringify;
use js::jsapi::OnNewGlobalHookOption;
use js::jsapi::ToJSONMaybeSafely;
use js::jsapi::Type;
use js::jsapi::Value;
use js::jsval::ObjectOrNullValue;
use js::jsval::StringValue;
use js::jsval::UndefinedValue;
use js::rust::{Runtime, SIMPLE_GLOBAL_CLASS};
use js::typedarray::{CreateWith, Int8Array, Uint8Array, Uint8ClampedArray};

use std::ffi::{CStr, CString};
use std::fmt;
use std::io::Read;
use std::io::Write;
use std::os::raw::c_void;
use std::os::unix::net::UnixStream;
use std::ptr;
use std::slice;
use std::str;


// This might be what i need:
// https://github.com/modrzew/hnfd/blob/7826be05cfe0c83aec43ceeb50d3362655b9d800/src/server.rs

// Each client has an ID that come from  `RadioInterfaceLayer()` in
// `dom/system/gonk/RadioInterfaceLayer.js` and is an unsigned integer. There's a client
// for every radio interface. That client ID corresponds to the index position within
// this vector
thread_local!(static ril_clients: RefCell<Vec<Option<Client>>> = RefCell::new(Vec::new()));

/// The name to the RIL daemon. This is just the prefix if more than one modem is there. The
/// first one is just called `rild`, the next one is called `rild2` (and so on).
/// FirefoxOS used a proxy (located at /dev/socket/rilproxy) due to the priviledges needed, we
/// can directly connect to the RIL daemon.
const RIL_SOCKET_NAME: &'static str = "/dev/socket/rild";
//const RIL_SOCKET_NAME: &'static str = "/home/vmx/src/rust/b2g/ril/socketserver/uds_socket";

// The source of the RIL JS Worker
//const JS_WORKER_FILE: &'static str = "/home/vmx/src/rust/b2g/ril/websocket/websocket/js/all.js";
const JS_WORKER_FILE: &'static str = "./js/all.js";

//#[derive(Debug)]
struct RegisterMessage {
    client_id: u8,
    raw_message: String,
    websocket_sender: ws::Sender,
}

impl fmt::Debug for RegisterMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f,
               "RegisterMessage {{ client_id: {}, raw_message: {} }}",
               self.client_id,
               self.raw_message)
    }
}

// It's name "kind" instead of "type" as "type" is a reserved word
#[derive(Deserialize, Debug)]
enum MessageKind {
    #[serde(rename = "registerClient")]
    Register,
    #[serde(rename = "setRadioEnabled")]
    SetRadioEnabled,
}

/// The deserialized message received as JSON via the WebSocket
#[derive(Deserialize, Debug)]
struct WebsocketMessage {
    // The client ID comes from the  `RadioInterfaceLayer()` in
    // `dom/system/gonk/RadioInterfaceLayer.js` and is an unsigned integer. There's a client
    // for every radio interface.
    // NOTE vmx 2017-02-12: I don't expect more than 256 radio interfaces on one device
    #[serde(rename = "rilMessageClientId")]
    client_id: Option<u8>,
    // `clientId` is only given if it's a register message
    #[serde(rename = "clientId")]
    register_client_id: Option<u8>,
    #[serde(rename = "rilMessageToken")]
    token: u64,
    #[serde(rename = "rilMessageType")]
    kind: MessageKind,
}

#[derive(Debug)]
struct RilMessage {
    client_id: u8,
    data: Vec<u8>,
}

#[derive(Debug)]
enum Message {
    // Registering a client is a special case. Nit only the JS RIL Worker needs the message, but
    // we also need to store the WebSocket it is using, so that we have proper bi-directional
    // communication with the right client
    Register(RegisterMessage),
    // The messages we are receiving from the WebSocket are just forwarded to the JS Worker
    WebsocketMessage(String),
    // Data from the RIL Socket
    RilMessage(RilMessage),
}

//#[derive(Debug)]
struct Client {
    ril_socket: RilSocket,
    websocket_sender: ws::Sender,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Client {{ ril_socket: {:?} }}", self.ril_socket)
    }
}


impl Client {
    // TODO vmx 2017-03-02: Add a proper Error struct
    fn send(&mut self, context: *mut JSContext, args: &CallArgs) -> Result<(), String> {
        println!("Trying to send a message to the RIL Daemon");

        // NOTE vmx 2017-03-05: The original FirefoxOS code checks if the socket connection is
        // still there and returns OK if it isn't as it is assumed that it is shutting down.

        let value = args.get(1);
        println!("value: {:?}", value);
        let raw = if value.is_string() {
            println!("value: is string {:?}", value);

            // NOTE vmx 3017-03-05: This code is from a Servo example, perhaps there's an
            // easier way.
            unsafe {
                let str = js::rust::ToString(context, value);
                rooted!(in(context) let message_root = str);
                let message = JS_EncodeStringToUTF8(context, message_root.handle());
                let message = CStr::from_ptr(message);
                println!("{}", str::from_utf8(message.to_bytes()).unwrap());
                //let raw = str::from_utf8(message.to_bytes()).unwrap();
                //str::from_utf8(message.to_bytes()).unwrap();
                message.to_bytes()
            }
        } else if !value.is_primitive() {
            println!("value: not a primitive {:?}", value);
            let obj = value.to_object_or_null();
            unsafe {
                if !JS_IsTypedArrayObject(obj) {
                    return Err("Object passed in wasn't a typed array".to_string());
                }

                let view_type = unsafe{ JS_GetArrayBufferViewType(obj) };
                if view_type != Type::Int8 &&
                        view_type != Type::Uint8 &&
                        view_type != Type::Uint8Clamped {
                    return Err("Typed array data is not octets".to_string());
                }

                let size = JS_GetTypedArrayByteLength(obj);
                let mut is_shared = false;
                let nogc = AutoCheckCannotGC{
                    _base: AutoAssertOnGC{}
                };
                let data = JS_GetArrayBufferViewData(obj,
                                                     &mut is_shared as *mut bool,
                                                     &nogc  as *const _);
                if is_shared {
                    return Err("Incorrect argument. Shared memory not supported".to_string());
                }

                // NOTE vmx 2017-03-05: I'm not sure if casting to `*const u8` will always be
                // the right thing.
                slice::from_raw_parts(data as *const u8, size as usize)
            }
        } else {
            return Err("Incorrect argument. Expecting a string or a typed array".to_string());
        };
        println!("raw: {:?}", raw);

        self.ril_socket.send(raw);
        Ok(())
    }
}


// `RilSocket` is just a light wrapper around the Unix Socket communicating with the RIL deamons
#[derive(Debug)]
struct RilSocket {
    client_id: u8,
    socket: UnixStream,
    //message_sender: mpsc::Sender<Message>,
}

impl RilSocket {
    fn new(client_id: u8, message_sender: mpsc::Sender<Message>) -> RilSocket {
        // The RIL daemons are named `rild`, `rild2`, `rild3`...
        let address = match client_id {
            0 => RIL_SOCKET_NAME.to_string(),
            _ => format!("{}{}", RIL_SOCKET_NAME, client_id),
        };
        // TODO vmx 2017-03-05: Add proper error handling
        let socket = UnixStream::connect(address).unwrap();

        let mut read_socket = socket.try_clone().unwrap();
        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            loop {
                let num_bytes = read_socket.read(&mut buf).unwrap();
                println!("Read {} bytes of the socket: {:?}", num_bytes, &buf[..num_bytes]);
                //XXX vmx 2017-03-13: GO ON HERE and send the data over a channel to the JS worker. Do the same as in Ril.cpp RilConsumer::Receive()
                message_sender.send(Message::RilMessage(RilMessage{
                    client_id: client_id,
                    data: (&buf[..num_bytes]).to_vec(),
                }));
            }
        });


        RilSocket{
            client_id: client_id,
            socket: socket,
        }
    }

    fn send(&mut self, data: &[u8]) {
        self.socket.write_all(data).unwrap();
    }
}





struct JsWorker {
    // The receiver is needed to get messages from the WebSocket
    message_receiver: mpsc::Receiver<Message>,
    // The sender is needed to send messages from the RIL Socket to the JS Worker
    message_sender: mpsc::Sender<Message>,

    // NOTE vmx 2017-02-15: Perhaps I want to refactor this into a new struct that contains
    // all the JS related stuff
    // For the JS stuff
    js_context: Option<*mut JSContext>,
    js_global: Option<HandleObject>,
    js_runtime: Option<Runtime>,
}

impl JsWorker {
    fn run(&mut self) {
        println!("JS Worker got started!");

        //let js_worker_file = OpenOptions::new().read(true).open(JS_WORKER_SOURCE_FILE).unwrap();
        let mut js_worker_file = File::open(JS_WORKER_FILE).unwrap();
        let mut js_worker_source = String::new();
        js_worker_file.read_to_string(&mut js_worker_source).unwrap();


        let runtime = Runtime::new().unwrap();
        let context = runtime.cx();
        let h_option = OnNewGlobalHookOption::FireOnNewGlobalHook;
        let c_option = CompartmentOptions::default();

        unsafe {
            let global2 = JS_NewGlobalObject(context,
                                             &SIMPLE_GLOBAL_CLASS,
                                             ptr::null_mut(),
                                             h_option,
                                             &c_option);
            rooted!(in(context) let global_root = global2);
            let global = global_root.handle();
            let _ac = JSAutoCompartment::new(context, global.get());
            let post_ril_message = JS_DefineFunction(context,
                                                     global,
                                                     b"postRILMessage\0".as_ptr() as *const _,
                                                     Some(post_ril_message),
                                                     2,
                                                     0);
            assert!(!post_ril_message.is_null());
            let post_message = JS_DefineFunction(context,
                                                 global,
                                                 b"postMessage\0".as_ptr() as *const _,
                                                 Some(post_message),
                                                 1,
                                                 0);
            assert!(!post_message.is_null());
            rooted!(in(context) let mut rval = UndefinedValue());
            let _ = runtime.evaluate_script(global,
                                            &js_worker_source,
                                            "all.js",
                                            0,
                                            rval.handle_mut());


            println!("JS Worker is waiting for a message!");

            while let Ok(message) = self.message_receiver.recv() {
                println!("JS Worker received a message: {:?}", message);
                //let raw_message = match message {
                match message {
                    Message::Register(RegisterMessage{client_id, raw_message, websocket_sender}) => {
                        self.handle_register(client_id, websocket_sender);
                        self.send_event(context, global, &raw_message);
                    },
                    Message::WebsocketMessage(raw_message) => {
                        self.send_event(context, global, &raw_message);
                    },
                    Message::RilMessage(RilMessage{client_id, data}) => {
                        self.send_ril_message(context, global, client_id, &data);
                    },
                };

            }

        };
    }

    /// Trigger an event by calling `onmessage()` of the RIL JS Worker script
    unsafe fn send_event(&self, context: *mut JSContext, global: HandleObject, raw_message: &str) {
        let event = format!(r#"{{"data": {}}}"#, raw_message);
        println!("Sending event to JS RIL Worker: {}", event);
        let event: Vec<u16> = event.encode_utf16().collect();
        rooted!(in(context) let mut rooted_event_json = UndefinedValue());
        // NOTE vmx 2017-02-22: Error handling when JSON parsing failed  is missing (when false
        // is returned)
        JS_ParseJSON(context, event.as_ptr(), event.len() as u32, rooted_event_json.handle_mut());
        let args = vec![rooted_event_json.handle().get()];

        rooted!(in(context) let mut rval = UndefinedValue());
        JS_CallFunctionName(context, global, b"onmessage".as_ptr() as *const _,
                            &HandleValueArray {
                                length_: 1 ,
                                elements_: args.as_ptr(),
                            }, rval.handle_mut());

        let return_value = String::from_jsval(context, rval.handle(), ());
        println!("return value of the onmessage call: {:?}", return_value);
    }

    /// Send a RIL message to the RIL JS Worker script by calling `onRILMessage()`
    unsafe fn send_ril_message(&self, context: *mut JSContext,
                               global: HandleObject,
                               client_id: u8,
                               data: &[u8]) {
        println!("JS Worker is processing a message from the RIL socket");

        rooted!(in(context) let mut array = ptr::null_mut());
        Uint8Array::create(context, CreateWith::Slice(&data[..]), array.handle_mut()).is_ok();

        rooted!(in(context) let mut client_id_val = UndefinedValue());
        client_id.to_jsval(context, client_id_val.handle_mut());

        let args = vec![client_id_val.get(), ObjectOrNullValue(array.get())];
        let handle_value_array = HandleValueArray::from_rooted_slice(&args);
        rooted!(in(context) let mut not_used = UndefinedValue());
        JS_CallFunctionName(context, global, b"onRILMessage".as_ptr() as *const _,
                            &handle_value_array, not_used.handle_mut());
        let return_value = String::from_jsval(context, not_used.handle(), ());
        println!("Return value of onRILMessage: {:?}", return_value.unwrap());
    }

    /// Register a new client
    fn handle_register(&mut self, client_id: u8, websocket_sender: ws::Sender) {
        println!("JS Worker handling register");
        self.ensure_clients_length(client_id);
        let ril_socket = RilSocket::new(client_id, self.message_sender.clone());
        ril_clients.with(|client| (*client.borrow_mut())[client_id as usize] = Some(Client{
            ril_socket: ril_socket,
            websocket_sender: websocket_sender,
        }));
    }

    /// Make sure the vector has enough items to address it directly with an index
    ///
    /// This corresponds to `EnsureLengthAtLeast()` from Mozilla's `nsTArray`.
    fn ensure_clients_length(&mut self, client_id: u8) {
        for _ in ril_clients.with(|client| (*client.borrow()).len())..client_id as usize + 1 {
            ril_clients.with(|client| (*client.borrow_mut()).push(None));
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
            let worker_message: WebsocketMessage = serde_json::from_str(&json).unwrap();
            println!("received a message JSON parssed: {:?}", worker_message);
            let message = match worker_message.kind {
                MessageKind::Register => Message::Register(RegisterMessage{
                    client_id: worker_message.register_client_id.unwrap(),
                    raw_message: json,
                    websocket_sender: self.websocket_sender.clone(),
                }),
                _ => Message::WebsocketMessage(json),
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

// Write the stringified JSON into the string that was supplied as pointer to a Box
unsafe extern "C" fn stringify_json_callback(string_ptr: *const u16, len: u32, rval: *mut c_void) -> bool {
    let mut ret = Box::from_raw(rval as *mut String);
    let slice = slice::from_raw_parts(string_ptr, len as usize);
    ret.push_str(String::from_utf16(slice).unwrap().as_str());
    // Without this line the pointer can't be converted back into a box after this function was called
    Box::into_raw(ret);
    true
}

// Process a message from the script that is hooked up with the RIL daemon and send it over the
// WebSocket to Firefox
unsafe extern "C" fn post_message(context: *mut JSContext, argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, argc);

    if args._base.argc_ != 1 {
        JS_ReportError(context, b"Expecting one argument as message\0".as_ptr() as *const _);
        return false;
    }

    let arg = args.get(0);
    rooted!(in(context) let arg = arg.to_object());

    let message = Box::new(String::new());
    let message_ptr = Box::into_raw(message);
    let did_the_stringify_work = ToJSONMaybeSafely(context,
                                                   arg.handle(),
                                                   Some(stringify_json_callback),
                                                   message_ptr as *mut c_void);
    println!("did the stringify work?: {:?}", did_the_stringify_work);
    let message = *Box::from_raw(message_ptr);
    println!("The return valud of stringify is: {:?}", message);

    let parsed_message: serde_json::Value = serde_json::from_str(&message).unwrap();
    let client_id = parsed_message["rilMessageClientId"].as_i64().unwrap();
    println!("client ID is: {}", client_id);

    ril_clients.with(|cc| {
        let ref client = (*cc.borrow())[client_id as usize];
        println!("Do I have access to the thread-local clients? {:?}", client);
        client.as_ref().unwrap().websocket_sender.send(message);
    });
    true
}


// Process a message from the JS worker script and forward it to the RIL Daemon
unsafe extern "C" fn post_ril_message(context: *mut JSContext, argc: u32, vp: *mut Value) -> bool {
    println!("Rust post_rill_message got called");
    let args = CallArgs::from_vp(vp, argc);

    if args._base.argc_ != 2 {
        JS_ReportError(context, b"Expecting two arguments as message\0".as_ptr() as *const _);
        return false;
    }

    //let return_value = String::from_jsval(context, rval.handle(), ());
    //println!("return value of the onmessage call: {:?}", return_value);

    //let client_id = args.get(0);
    ////rooted!(in(context) let client_id = client_id.get().to_int32());
    //let client_id = client_id.to_int32();
    let client_id = args.get(0).to_int32();
    //rooted!(in(context) let client_id = UndefinedValue());
    //let client_id = i32::from_jsval(context, cleint_id.handle, ());


    ril_clients.with(|cc| {
        let ref mut clients = *cc.borrow_mut();
        if clients.len() <= client_id as usize || clients[client_id as usize].is_none() {
            // Probably shutting down.
            return true;
        }
        let ret = clients[client_id as usize].as_mut().unwrap().send(context, &args);
        ret.is_ok()
    })
}




fn main() {
    let (sender, receiver): (mpsc::Sender<Message>, mpsc::Receiver<Message>) = mpsc::channel();

    let sender_for_jsworker = sender.clone();
    thread::spawn(move || {
        let mut js_worker = JsWorker {
            message_receiver: receiver,
            message_sender: sender_for_jsworker,
            js_context: None,
            js_global: None,
            js_runtime: None,
        };

        js_worker.run();
    });

    listen("127.0.0.1:3012", |websocket_sender| {
        WebsocketServer {
            websocket_sender: websocket_sender,
            message_sender: sender.clone(),
        }
    }).unwrap()
}
