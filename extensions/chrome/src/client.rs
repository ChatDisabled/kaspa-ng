use crate::imports::*;
// use kaspa_ng_core::interop; //::transport;
use kaspa_ng_core::interop::Client;

pub struct ClientReceiver {
    _sender: Arc<dyn interop::Sender>,
    client: Arc<Client>,
    application_events: ApplicationEventsChannel,
    closure: Mutex<Option<Rc<ListenerClosure>>>,
    chrome_extension_id: String,
}

unsafe impl Send for ClientReceiver {}
unsafe impl Sync for ClientReceiver {}

impl ClientReceiver {
    pub fn new(
        _sender: Arc<dyn interop::Sender>,
        client: Arc<Client>,
        application_events: ApplicationEventsChannel,
    ) -> Self {
        Self {
            _sender,
            client,
            application_events,
            chrome_extension_id: runtime_id().unwrap(),
            closure: Mutex::new(None),
        }
    }

    pub fn start(self: &Arc<Self>) {
        self.register_listener();
    }

    fn register_listener(self: &Arc<Self>) {
        let this = self.clone();

        let closure = Rc::new(Closure::new(
            move |msg, sender: Sender, _callback: JsValue| -> JsValue {
                log_info!("CLIENT RECEIVED MESSAGE: {:?}", msg);
                if let Err(err) = this.handle_notification(msg, sender) {
                    log_error!("notification handling error: {:?}", err);
                }
                JsValue::from(false)
            },
        ));

        log_info!("CLIENT REGISTERING LISTENER...");
        chrome_runtime_on_message::add_listener(closure.clone().as_ref());
        *self.closure.lock().unwrap() = Some(closure);
    }

    fn handle_notification(
        self: &Arc<Self>,
        msg: JsValue,
        sender: Sender,
        // callback: Function,
    ) -> Result<()> {
        log_info!("CLIENT HANDLING NOTIFICATION...");
        if let Some(id) = sender.id() {
            if id != self.chrome_extension_id {
                return Err(Error::custom(
                    "Unknown sender id - foreign requests are forbidden",
                ));
            }
        } else {
            return Err(Error::custom("Sender is missing id"));
        }

        log_info!(
            "[WASM] notification: {:?}, sender id:{:?}",
            msg,
            sender.id(),
            // callback
        );

        let (target, data) = jsv_to_notify(msg)?;

        match target {
            Target::Wallet => {
                let event = Box::new(kaspa_wallet_core::events::Events::try_from_slice(&data)?);

                self.application_events
                    .sender
                    .try_send(kaspa_ng_core::events::Events::Wallet { event })
                    .unwrap();
            }
            _ => {
                let self_ = self.clone();
                spawn_local(async move {
                    if let Err(err) = self_.client.handle_message(target, 0, data).await {
                        log_error!("error handling message: {:?}", err);
                    }
                });
            }
        }

        Ok(())
    }
}
