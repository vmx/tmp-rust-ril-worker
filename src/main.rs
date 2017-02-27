#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate ws;

use std::cell::RefCell;
use std::thread;
use std::sync::mpsc;

use ws::listen;



#[macro_use] extern crate js;

use js::conversions::{FromJSValConvertible, ToJSValConvertible};
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
use js::jsapi::JS_NewGlobalObject;
use js::jsapi::JS_ParseJSON;
use js::jsapi::JS_ParseJSON1;
use js::jsapi::JS_ReportError;
use js::jsapi::JS_Stringify;
use js::jsapi::OnNewGlobalHookOption;
use js::jsapi::ToJSONMaybeSafely;
use js::jsapi::Value;
use js::jsval::UndefinedValue;
use js::jsval::StringValue;
use js::rust::{Runtime, SIMPLE_GLOBAL_CLASS};

use std::ffi::{CStr, CString};
use std::io::Write;
use std::os::raw::c_void;
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
struct WorkerMessage {
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
struct RegisterMessage {
    client_id: u8,
    raw_message: String,
    websocket_sender: ws::Sender,
}

#[derive(Debug)]
enum Message {
    // Registering a client is a special case. Nit only the JS RIL Worker needs the message, but
    // we also need to store the WebSocket it is using, so that we have proper bi-directional
    // communication with the right client
    Register(RegisterMessage),
    // The messages we are receiving are just forwarded to the JS Worker
    WorkerMessage(String),
}

#[derive(Debug)]
struct Client {
    websocket_sender: ws::Sender,
}


struct JsWorker {
    message_receiver: mpsc::Receiver<Message>,

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



    let runtime = Runtime::new();
    let context = runtime.cx();
    let h_option = OnNewGlobalHookOption::FireOnNewGlobalHook;
    let c_option = CompartmentOptions::default();

    unsafe {
        let global2 = JS_NewGlobalObject(context, &SIMPLE_GLOBAL_CLASS, ptr::null_mut(), h_option, &c_option);
        rooted!(in(context) let global_root = global2);
        let global = global_root.handle();
        let _ac = JSAutoCompartment::new(context, global.get());
        //let function = JS_DefineFunction(context, global, b"postRILMessage\0".as_ptr() as *const _,
        //                                 Some(post_ril_message), 2, 0);
        //assert!(!function.is_null());
        let post_message = JS_DefineFunction(context, global, b"postMessage\0".as_ptr() as *const _,
                                         Some(post_message), 1, 0);
        assert!(!post_message.is_null());
        //let javascript = "postRILMessage(0, new Uint8Array([1, 2, 3]));";
        //let javascript = "postRILMessage(123, {\"foo\": [1,2]});";
        //let javascript = "postRILMessage(123, [1,2]);";
        //let javascript = "var hello = function() {postRILMessage(0, new Uint8Array([1, 2, 3]));};";
        //let javascript = r#"var hello = function() {postMessage({"some": "data"});};"#;
//        let javascript = r#"postMessage({"some": "data"});"#;
        let javascript = r#"var onmessage = function(event) { return JSON.stringify(event.data); };"#;
        rooted!(in(context) let mut rval = UndefinedValue());
        let _ = runtime.evaluate_script(global, javascript, "test.js", 0, rval.handle_mut());
        //JS_CallFunctionName(context, global, b"hello".as_ptr() as *const _,
        //                    &HandleValueArray {
        //                        length_: 0 as _,
        //                        elements_: ptr::null_mut()
        //                    }, rval.handle_mut());
        //self.js_global = Some(global);

//    };
    //self.js_context = Some(context);
    //self.js_global = Some(global);
    //self.js_runtime = Some(runtime);


                    //let javascript = r#"postMessage({"some": "data"});"#;
                    //rooted!(in(context) let mut rval = UndefinedValue());
                    //let _ = runtime.evaluate_script(
                    //    global, javascript, "test.js", 0, rval.handle_mut());
                    //println!("Called postMessage()");



        println!("JS Worker is waiting for a message!");

        //XXX vmx 2017-02-08: GO ON HERE and send the websocket along, so that we can send
        //    messages async. It would make sense to have a loop below that is waiting for
        //    different kind of messages. One message will be sent by the websocket on
        //    the connection start (which will include the websocket itself). Others will
        //    be sent by postMessage() to actually send something back over the socket

        while let Ok(message) = self.message_receiver.recv() {
            println!("JS Worker received a message: {:?}", message);
            let raw_message = match message {
                Message::Register(RegisterMessage{client_id, raw_message, websocket_sender}) => {
                    self.handle_register(client_id, websocket_sender);
                    raw_message
                },
                Message::WorkerMessage(raw_message) => {
                    raw_message
                    ////XXX vmx GO ON HERE 2017-02-13: This is the message the JavaScript context
                    ////    gets via `onmessage`.
                    //println!("JS Worker received a ril message");
                    // 
                    //let javascript = r#"postMessage({"some": "data"});"#;
                    ////rooted!(in(self.js_context.unwrap()) let mut rval = UndefinedValue());
                    ////let _ = self.js_runtime.as_ref().unwrap().evaluate_script(
                    ////    self.js_global.unwrap(), javascript, "test.js", 0, rval.handle_mut());
                    //rooted!(in(context) let mut rval = UndefinedValue());
                    //let _ = runtime.evaluate_script(
                    //    global, javascript, "test.js", 0, rval.handle_mut());
                    //println!("Called postMessage()");
                },
            };



            let javascript = r#"postMessage({"rilMessageClientId": 0, "some": "data4"});"#;
            //rooted!(in(self.js_context.unwrap()) let mut rval = UndefinedValue());
            //let _ = self.js_runtime.as_ref().unwrap().evaluate_script(
            //    self.js_global.unwrap(), javascript, "test.js", 0, rval.handle_mut());
            rooted!(in(context) let mut rval = UndefinedValue());
            let _ = runtime.evaluate_script(
                global, javascript, "test.js", 0, rval.handle_mut());
            println!("Called postMessage()");





            let event = format!(r#"{{"data": {}}}"#, raw_message);
            println!("Sending event to JS RIL Worker: {}", event);
            let event: Vec<u16> = event.encode_utf16().collect();
            rooted!(in(context) let mut rooted_event_json = UndefinedValue());
            // NOTE vmx 2017-02-22: Error handle is missing (when false is returned)
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

            };
    }

    /// Register a new client
    fn handle_register(&mut self, client_id: u8, websocket_sender: ws::Sender) {
        println!("JS Worker handling register");
        self.ensure_clients_length(client_id);
        ril_clients.with(|client| (*client.borrow_mut())[client_id as usize] = Some(Client{
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
            let worker_message: WorkerMessage = serde_json::from_str(&json).unwrap();
            println!("received a message JSON parssed: {:?}", worker_message);
            let message = match worker_message.kind {
                MessageKind::Register => Message::Register(RegisterMessage{
                    client_id: worker_message.register_client_id.unwrap(),
                    raw_message: json,
                    websocket_sender: self.websocket_sender.clone(),
                }),
                _ => Message::WorkerMessage(json),
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


//fn(_: *const c_ushort, _: u32, _: *mut c_void)
//let event: Vec<u16> = event.encode_utf16().collect();
//JS_ParseJSON(context, event.as_ptr(), event.len() as u32, rooted_event_json.handle_mut());
unsafe extern "C" fn stringify_json_callback(string_ptr: *const u16, len: u32, rval: *mut c_void) -> bool {
    let mut ret = Box::from_raw(rval as *mut String);
    let slice = slice::from_raw_parts(string_ptr, len as usize);
    ret.push_str(String::from_utf16(slice).unwrap().as_str());
    // Without this line the pointer can't be converted back into a box after this function was called
    Box::into_raw(ret);
    true
}
//unsafe extern "C" fn wrap(cx: *mut JSContext,
//                          _existing: HandleObject,
//                          obj: HandleObject)
//                          -> *mut JSObject {
//    // FIXME terrible idea. need security wrappers
//    // https://github.com/servo/servo/issues/2382
//    WrapperNew(cx, obj, GetCrossCompartmentWrapper(), ptr::null(), false)
//}


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

//+    if (!Stringify(cx, &inputValue, nullptr, NullHandleValue, sb,
//+                   StringifyBehavior::RestrictedSafe))
    //rooted!(in(context) let mut message = UndefinedValue());
    //let mut message = String::new();
    let message = Box::new(String::new());
    let message_ptr = Box::into_raw(message);
    //message.reserve(1000);
    //message.push_str("bar");
    //rooted!(in(context) let mut foo = arg.to_object());
    //let did_the_stringify_work = JS_Stringify(context,
    //                                          arg.handle_mut(),
    //                                          ptr::null,
    //                                          NullHandleValue,
    //                                          Some(stringify_json_cb),
    //                                          message.handle_mut());
    let did_the_stringify_work = ToJSONMaybeSafely(context,
                                                   arg.handle(),
                                                   Some(stringify_json_callback),
                                                   //message_ptr);
                                                   message_ptr as *mut c_void);
                                                   //message as *mut _ as *mut c_void);
                                                   //&mut message.handle_mut() as *mut _ as *mut c_void);
                                                   //message.handle_mut() as *mut _ as *mut c_void);
    println!("did the stringify work?: {:?}", did_the_stringify_work);
    let message = *Box::from_raw(message_ptr);
    println!("The return valud of stringify is: {:?}", message);

    //let foo = Box::into_raw(message2);
    //let message3 = unsafe { Box::from_raw(foo) };
    //println!("The return valud of stringify is: {:?}", message3);

////    let js = js::rust::ToString(context, arg);
//    rooted!(in(context) let message_root = js);
//    let message = JS_EncodeStringToUTF8(context, message_root.handle());
//    let message = CStr::from_ptr(message);
// 
//    println!("I want to get passed on via WebSocket to Firefox: {}",
//             str::from_utf8(message.to_bytes()).unwrap());
    //XXX vmx 2017-02-13: GO ON HERE and parse the JSON with serde_json into a Rust struct. That
    //    then contains the client_id (rilMessageClientId) which is then used to produce a message
    //    that is send over the channel to the JS Worker. The JS Worker know the clients and will
    //    then use the correct WebSocket to send off that message.
    //    The problem that needs to be solved here is that `post_message` needs to have a channel
    //    to the JS Worker.
    let parsed_message: serde_json::Value = serde_json::from_str(&message).unwrap();
    let client_id = parsed_message["rilMessageClientId"].as_i64().unwrap();
    println!("client ID is: {}", client_id);

    ril_clients.with(|cc| {
        let ref client = (*cc.borrow())[client_id as usize];
        println!("Do I have access to the thread-local clients? {:?}", client);
        //client.as_ref().unwrap().websocket_sender.send(r#"{"Hello": "World!"}"#);
        client.as_ref().unwrap().websocket_sender.send(message);
    });
    true
}




fn main() {
    let (sender, receiver): (mpsc::Sender<Message>, mpsc::Receiver<Message>) = mpsc::channel();

    thread::spawn(move || {
        let mut js_worker = JsWorker {
            message_receiver: receiver,
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
