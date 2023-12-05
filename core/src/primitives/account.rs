use kaspa_wallet_core::storage::AccountKind;

use crate::imports::*;

pub struct AccountContext {
    qr: load::Bytes,
    receive_address: Address,
    uri: String,
}

impl AccountContext {
    pub fn new(descriptor: &AccountDescriptor) -> Option<Arc<Self>> {
        if let Some(receive_address) = descriptor.receive_address() {
            let address_string = receive_address.to_string();
            let qr = render_qrcode(&address_string, 128, 128);
            let uri = format!("bytes://{}-{}.svg", address_string, theme_color().name);
            Some(Arc::new(Self {
                qr: qr.as_bytes().to_vec().into(),
                receive_address,
                uri,
            }))
        } else {
            None
        }
    }

    pub fn address(&self) -> &Address {
        &self.receive_address
    }

    pub fn qr(&self) -> load::Bytes {
        self.qr.clone()
    }

    pub fn uri(&self) -> String {
        self.uri.clone()
    }
}

struct Inner {
    // runtime: Arc<dyn runtime::Account>,
    id: AccountId,
    balance: Mutex<Option<Balance>>,
    descriptor: Mutex<AccountDescriptor>,
    context: Mutex<Option<Arc<AccountContext>>>,
    transactions: Mutex<TransactionCollection>,
    total_transaction_count: AtomicU64,
    is_loading: AtomicBool,
    // for bip32 accounts only
    bip39_passphrase: bool,
}

impl Inner {
    fn new(descriptor: AccountDescriptor) -> Self {
        let bip39_passphrase = match &descriptor {
            AccountDescriptor::Bip32(bip32) => bip32.bip39_passphrase,
            _ => false,
        };

        let context = AccountContext::new(&descriptor);
        Self {
            id: *descriptor.account_id(),
            balance: Mutex::new(None),
            descriptor: Mutex::new(descriptor),
            context: Mutex::new(context),
            transactions: Mutex::new(TransactionCollection::default()),
            total_transaction_count: AtomicU64::new(0),
            is_loading: AtomicBool::new(true),
            bip39_passphrase,
        }
    }
}

#[derive(Clone)]
pub struct Account {
    inner: Arc<Inner>,
}

impl From<AccountDescriptor> for Account {
    fn from(descriptor: AccountDescriptor) -> Self {
        Self {
            inner: Arc::new(Inner::new(descriptor)),
        }
    }
}

impl Account {
    pub fn descriptor(&self) -> MutexGuard<'_, AccountDescriptor> {
        self.inner.descriptor.lock().unwrap()
    }

    pub fn receive_address(&self) -> Address {
        self.inner
            .context
            .lock()
            .unwrap()
            .as_ref()
            .map(|context| context.address().clone())
            .unwrap()
    }

    pub fn transactions(&self) -> MutexGuard<'_, TransactionCollection> {
        self.inner.transactions.lock().unwrap()
    }

    pub fn id(&self) -> AccountId {
        self.inner.id
    }

    pub fn name_or_id(&self) -> String {
        self.descriptor().name_or_id()
    }

    pub fn requires_bip39_passphrase(&self) -> bool {
        self.inner.bip39_passphrase
    }

    pub fn balance(&self) -> Option<Balance> {
        self.inner.balance.lock().unwrap().clone()
    }

    pub fn update_theme(&self) {
        let descriptor = self.descriptor().clone();
        *self.inner.context.lock().unwrap() = AccountContext::new(&descriptor);
    }

    pub fn context(&self) -> Option<Arc<AccountContext>> {
        self.inner.context.lock().unwrap().clone()
    }

    pub fn update(&self, descriptor: AccountDescriptor) {
        *self.inner.context.lock().unwrap() = AccountContext::new(&descriptor);
        *self.inner.descriptor.lock().unwrap() = descriptor;
    }

    pub fn update_balance(&self, balance: Option<Balance>) -> Result<()> {
        *self.inner.balance.lock().unwrap() = balance;

        Ok(())
    }

    pub fn set_loading(&self, is_loading: bool) {
        self.inner.is_loading.store(is_loading, Ordering::SeqCst);
    }

    pub fn is_loading(&self) -> bool {
        self.inner.is_loading.load(Ordering::SeqCst)
    }

    pub fn set_transaction_count(&self, count: u64) {
        self.inner
            .total_transaction_count
            .store(count, Ordering::SeqCst);
    }

    pub fn transaction_count(&self) -> u64 {
        self.inner.total_transaction_count.load(Ordering::SeqCst)
    }

    pub fn load_transactions(
        &self,
        transactions: Vec<Arc<TransactionRecord>>,
        total: u64,
    ) -> Result<()> {
        self.set_transaction_count(total);
        self.transactions()
            .load(transactions.into_iter().map(|t| t.into()));

        Ok(())
    }
}

impl IdT for Account {
    type Id = AccountId;

    fn id(&self) -> &Self::Id {
        &self.inner.id
    }
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.id(), f)
    }
}

pub type AccountCollection = Collection<AccountId, Account>;

pub trait DescribeAccount {
    fn describe(&self) -> (&'static str, &'static str);
}

impl DescribeAccount for AccountKind {
    fn describe(&self) -> (&'static str, &'static str) {
        match self {
            AccountKind::Legacy => ("Legacy Account", "KDX, PWA (kaspanet.io)"),
            AccountKind::Bip32 => ("Kaspa Core BIP32", "kaspawallet, kaspium"),
            AccountKind::MultiSig => ("Multi-Signature", ""),
            AccountKind::Keypair => ("Keypair", "secp256k1"),
            AccountKind::Hardware => ("Hardware", ""),
            _ => ("", ""),
        }
    }
}

pub trait AccountSelectorButtonExtension {
    fn account_selector_button(
        &mut self,
        account: &Account,
        network_type: &NetworkType,
        selected: bool,
    ) -> Response;
}

impl AccountSelectorButtonExtension for Ui {
    fn account_selector_button(
        &mut self,
        account: &Account,
        network_type: &NetworkType,
        selected: bool,
    ) -> Response {
        let account_name = account.name_or_id();

        let icon = if selected {
            Composite::icon(egui_phosphor::thin::CHECK)
        } else {
            Composite::icon(egui_phosphor::thin::LIST_DASHES)
        };

        let large_button_size = theme_style().large_button_size() + vec2(32., 0.);
        if let Some(balance) = account.balance() {
            let color = self.style().visuals.text_color();
            self.add_sized(
                large_button_size,
                CompositeButton::image_and_text(
                    icon,
                    RichText::new(account_name).size(14.),
                    s2kws_layout_job(balance.mature, network_type, color, FontId::monospace(16.)),
                ),
            )
        } else {
            self.add_sized(
                large_button_size,
                CompositeButton::image_and_text(
                    icon,
                    RichText::new(account_name).size(14.),
                    RichText::new("N/A").font(FontId::monospace(16.)),
                ),
            )
        }
    }
}
