use crate::imports::*;
use crate::primitives::account;
use std::borrow::Cow;
use egui_phosphor::thin::{CLOUD_ARROW_DOWN, CLOUD_SLASH};
use kaspa_wallet_core::tx::{GeneratorSummary, PaymentOutput, Fees};
use kaspa_wallet_core::api::*;
use crate::primitives::descriptors::*;

mod address;
mod balance;
mod destination;
mod details;
mod estimator;
mod menus;
mod network;
mod overview;
mod processor;
mod qr;
mod secret;
mod transactions;
mod transfer;
mod utxo;

use address::*;
use balance::*;
use destination::*;
use details::*;
use estimator::*;
use menus::*;
use network::*;
use overview::*;
use processor::*;
use qr::*;
use secret::*;
use transactions::*;
use transfer::*;
use utxo::*;


#[allow(dead_code)]
#[derive(Clone)]
enum AccountManagerState {
    Select,
    Overview { account: Account },
}

#[derive(Default, Copy, Clone, Eq, PartialEq)]
pub enum AccountManagerSection {
    #[default]
    // None,
    Overview,
    Transactions,
    Details,
    UtxoManager
}

// #[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
// #[derive(Default, Debug, Clone, Eq, PartialEq)]
#[derive(Default, Debug, Clone)]
enum Action {
    #[default]
    None,
    Estimating,
    Sending,
    // Reset,
    Processing,
    Error(Arc<Error>),
}


// impl Action {
//     fn is_sending(&self) -> bool {
//         matches!(self, Action::Sending | Action::Estimating | Action::Processing)
//     }
// }

#[derive(Clone, Copy, Eq, PartialEq)]
enum TransactionKind {
    Send,
    Transfer,
}



#[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
enum Focus {
    #[default]
    None,
    Address,
    Amount,
    Fees,
    WalletSecret,
    PaymentSecret,
}

#[derive(Default)]
pub enum EstimatorStatus {
    #[default]
    None,
    GeneratorSummary(GeneratorSummary),
    Error(String),
}

#[derive(Default, Clone, Eq, PartialEq)]
enum AddressStatus {
    #[default]
    None,
    Valid,
    NetworkMismatch(NetworkType),
    Invalid(String),
}

#[derive(Default)]
pub struct ManagerContext {
    transfer_to_account : Option<Account>,
    destination_address_string : String,
    send_amount_text: String,
    send_amount_sompi : u64,
    enable_priority_fees : bool,
    priority_fees_text : String,
    priority_fees_sompi : u64,
    estimate : Arc<Mutex<EstimatorStatus>>,
    request_estimate : Option<bool>,
    address_status : AddressStatus,
    action : Action,
    transaction_kind : Option<TransactionKind>,
    focus : FocusManager<Focus>,
    wallet_secret : String,
    payment_secret : String,
}

impl ManagerContext {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    fn reset_send_state(&mut self) {
        self.action = Action::None;

        self.zeroize()
    }
}

impl Zeroize for ManagerContext {
    fn zeroize(&mut self) {
        println!("*** resetting send state...");

        self.transfer_to_account = None;
        self.destination_address_string = String::default();
        self.send_amount_text = String::default();
        self.send_amount_sompi = 0;
        self.enable_priority_fees = false;
        self.priority_fees_text = String::default();
        self.priority_fees_sompi = 0;
        *self.estimate.lock().unwrap() = EstimatorStatus::None;
        self.address_status = AddressStatus::None;
        self.transaction_kind = None;
        self.focus.clear();
        self.wallet_secret.zeroize();
        self.payment_secret.zeroize();    
    }
}

pub struct RenderContext<'render> {
    pub account : &'render Account,
    pub context : Arc<account::AccountContext>,
    pub network_type : NetworkType,
    pub current_daa_score : Option<u64>,
}

impl<'render> RenderContext<'render> {
    pub fn new(account : &'render Account, network_type : NetworkType, current_daa_score : Option<u64>) -> Result<Self> {

        let context = if let Some(context) = account.context() {
            context
        } else {
            return Err(Error::custom("Account is missing context"));
        };

        Ok(Self {
            account,
            context,
            network_type,
            current_daa_score,
        })
    }
}

pub struct AccountManager {
    #[allow(dead_code)]
    runtime: Runtime,

    state: AccountManagerState,
    section: AccountManagerSection,
    context : ManagerContext,
}

impl AccountManager {
    pub fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
            state: AccountManagerState::Select,
            section: AccountManagerSection::Overview,
            context : ManagerContext::default(),
        }
    }
}

impl ModuleT for AccountManager {

    fn reset(&mut self, _core: &mut Core) {
        self.context = ManagerContext::default();
        self.state = AccountManagerState::Select;
    }

    // fn reload(&mut self, core : &mut Core) {
    //     if let AccountManagerState::Overview { account } = self.state.clone() {
    //         let account_id = account.id();
    //         core.account_collection().get()
    //     }
    // }

    fn render(
        &mut self,
        core: &mut Core,
        _ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        if let Err(err) = self.render_state(core, ui) {
            ui.colored_label(theme_color().error_color, err.to_string());
        }
    }

}

impl AccountManager {

    pub fn request_estimate(&mut self) {
        self.context.request_estimate = Some(true);
    }

    pub fn select(&mut self, account: Option<Account>, device : Device) {
        if let Some(account) = account {
            self.state = AccountManagerState::Overview {
                account: account.clone(),
            };
            
            if device.orientation() == Orientation::Portrait {
                self.section = AccountManagerSection::Overview;
            } else {
                self.section = AccountManagerSection::Transactions;
            }
        } else {
            self.state = AccountManagerState::Select;
        }

    }

    pub fn section(&mut self, section : AccountManagerSection) {
        self.section = section;
    }

    fn render_state(
        &mut self,
        core: &mut Core,
        ui: &mut egui::Ui,
    ) -> Result<()> {

        let network_type = if let Some(network_id) = core.state().network_id() {
            network_id.network_type()
        } else {
            core.settings.node.network.into()
        };

        let current_daa_score = core.state().current_daa_score();

        match self.state.clone() {
            AccountManagerState::Select => {

                if !core.state().is_open() {
                    core.select::<modules::WalletOpen>();
                } else if let Some(account_collection) = core.account_collection() {
                    if account_collection.is_empty() {
                        Panel::new(self)
                            .with_body(|_this, ui| {
                                ui.label("Please create an account");
                            }).render(ui);
                    } else if account_collection.len() == 1 {
                        self.select(Some(account_collection.first().unwrap().clone()), core.device().clone());
                    } else {
                        Panel::new(self)
                            .with_caption("Select Account")
                            .with_body(|this, ui| {

                                if !core.state().is_connected() {
                                    ui.label(
                                        RichText::new(CLOUD_SLASH)
                                            .size(theme_style().icon_size_large)
                                            .color(theme_color().icon_color_default)
                                    );
                                    ui.add_space(8.);                                    
                                    ui.label("You are currently not connected to the Kaspa node.");
                                    ui.add_space(16.);                                    
                                } else if !core.state().is_synced() {
                                    ui.label(
                                        RichText::new(CLOUD_ARROW_DOWN)
                                            .size(theme_style().icon_size_medium)
                                            .color(theme_color().icon_color_default)
                                    );
                                    ui.add_space(8.);
                                    ui.label("The node is currently syncing with the Kaspa p2p network. Account balances may be out of date.");
                                    ui.add_space(16.);
                                }

                                account_collection.iter().for_each(|account_select| {
                                    if ui.account_selector_button(account_select, &network_type, false).clicked() {
                                        this.select(Some(account_select.clone()), core.device().clone());
                                        if core.device().single_pane() {
                                            this.section = AccountManagerSection::Overview;
                                        } else {
                                            this.section = AccountManagerSection::Transactions;
                                        }
                                    }
                                });
                            }).render(ui);
                    }

                } else {
                    ui.label("Unable to access account list");
                }
            }

            AccountManagerState::Overview { account } => {
                let rc = RenderContext::new(&account, network_type, current_daa_score)?;
                if core.device().single_pane() {
                    self.render_singular_layout(core,ui,&rc, self.section);
                } else {
                    if self.section == AccountManagerSection::Overview {
                        self.section = AccountManagerSection::Transactions;
                    }
                    self.render_landscape(core,ui,&rc, self.section);
                }
            }
        }

        Ok(())
    }


    fn render_menu(&mut self, core: &mut Core, ui: &mut Ui, rc : &RenderContext<'_>) {
        ui.horizontal(|ui| {
            let screen_rect_height = ui.ctx().screen_rect().height();
            WalletMenu::new().render(core,ui,screen_rect_height * 0.8);
            ui.separator();
            AccountMenu::new().render(core,ui,self,rc, screen_rect_height * 0.8);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ToolsMenu::new().render(core,ui,self, rc, screen_rect_height * 0.8);

                ui.separator();

                if ui.add(Label::new("UTXOs").sense(Sense::click())).clicked() {
                    self.section = AccountManagerSection::UtxoManager;
                }
                ui.separator();
                if ui.add(Label::new("Details").sense(Sense::click())).clicked() {
                    self.section = AccountManagerSection::Details;
                }
                ui.separator();
                if ui.add(Label::new("Transactions").sense(Sense::click())).clicked() {
                    self.section = AccountManagerSection::Transactions;
                }

            });
        });
    }

    fn render_landscape(&mut self, core: &mut Core, ui: &mut Ui, rc : &RenderContext<'_>, section : AccountManagerSection) {

        let panel_width = ui.available_width() * 0.5;

        self.render_menu(core,ui,rc);

        SidePanel::left("account_manager_left")
            .exact_width(panel_width)
            .resizable(false)
            .show_separator_line(true)
            .show_inside(ui, |ui| {
            Overview::new(&mut self.context).render(core,ui,rc);
        });
        
        SidePanel::right("account_manager_right")
            .exact_width(panel_width)
            .resizable(false)
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                ui.separator();

                // ---
                ui.style_mut().text_styles = core.default_style.text_styles.clone();
                // ---

                match section {
                    AccountManagerSection::Overview => {
                        Overview::new(&mut self.context).render(core,ui,rc);
                    }
                    AccountManagerSection::Transactions => {
                        Transactions::new().render(ui,core,rc);
                    }
                    AccountManagerSection::Details => {
                        Details::new().render(core,ui,rc);
                    }
                    AccountManagerSection::UtxoManager => {
                        UtxoManager::new().render(core,ui,rc);
                    }
                }
            });


    }

    fn render_singular_layout(&mut self, core: &mut Core, ui: &mut Ui, rc : &RenderContext<'_>, section : AccountManagerSection) {

        match section {
            AccountManagerSection::Overview => {
                Overview::new(&mut self.context).render(core,ui,rc);
            }
            AccountManagerSection::Transactions => {
                Transactions::new().render(ui,core,rc);
            }
            AccountManagerSection::Details => {
                Details::new().render(core,ui,rc);
            }
            AccountManagerSection::UtxoManager => {
                UtxoManager::new().render(core,ui,rc);
            }
        }

    }

}