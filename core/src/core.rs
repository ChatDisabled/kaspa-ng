use crate::imports::*;
use crate::interop::Interop;
use crate::sync::SyncStatus;
use egui_notify::Toasts;
use kaspa_metrics::MetricsSnapshot;
use kaspa_wallet_core::events::Events as CoreWallet;
use kaspa_wallet_core::storage::{Binding, Hint};
use workflow_i18n::*;

const FORCE_WALLET_OPEN: bool = false;
const ENABLE_DUAL_PANE: bool = false;

enum Status {
    Connected {
        current_daa_score: Option<u64>,
        peers: Option<usize>,
        #[allow(dead_code)]
        tps: Option<f64>,
    },
    Disconnected,
    Syncing {
        sync_status: Option<SyncStatus>,
        peers: Option<usize>,
    },
}

pub enum Exception {
    UtxoIndexNotEnabled { url: Option<String> },
}

#[derive(Default)]
pub struct State {
    is_open: bool,
    is_connected: bool,
    is_synced: Option<bool>,
    sync_state: Option<SyncState>,
    server_version: Option<String>,
    url: Option<String>,
    network_id: Option<NetworkId>,
    current_daa_score: Option<u64>,
}

impl State {
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    pub fn is_synced(&self) -> bool {
        self.is_synced.unwrap_or(false) || matches!(self.sync_state, Some(SyncState::Synced))
    }

    pub fn sync_state(&self) -> &Option<SyncState> {
        &self.sync_state
    }

    pub fn server_version(&self) -> &Option<String> {
        &self.server_version
    }

    pub fn url(&self) -> &Option<String> {
        &self.url
    }

    pub fn network_id(&self) -> &Option<NetworkId> {
        &self.network_id
    }

    pub fn current_daa_score(&self) -> Option<u64> {
        self.current_daa_score
    }
}

pub struct Core {
    interop: Interop,
    wallet: Arc<dyn WalletApi>,
    channel: ApplicationEventsChannel,
    module: Module,
    stack: VecDeque<Module>,
    modules: HashMap<TypeId, Module>,
    pub settings: Settings,
    pub toasts: Toasts,
    pub large_style: egui::Style,
    pub default_style: egui::Style,
    pub metrics: Option<Box<MetricsSnapshot>>,

    state: State,
    hint: Option<Hint>,
    discard_hint: bool,
    exception: Option<Exception>,

    pub wallet_list: Vec<WalletDescriptor>,
    pub account_collection: Option<AccountCollection>,
    pub selected_account: Option<Account>,
}

impl Core {
    /// Core initialization
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        interop: crate::interop::Interop,
        mut settings: Settings,
    ) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Bold);
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Light);
        cc.egui_ctx.set_fonts(fonts);

        let mut default_style = (*cc.egui_ctx.style()).clone();

        default_style.text_styles.insert(
            TextStyle::Name("CompositeButtonSubtext".into()),
            FontId {
                size: 10.0,
                family: FontFamily::Proportional,
            },
        );

        let mut large_style = (*cc.egui_ctx.style()).clone();

        large_style.text_styles.insert(
            TextStyle::Name("CompositeButtonSubtext".into()),
            FontId {
                size: 12.0,
                family: FontFamily::Proportional,
            },
        );

        // println!("style: {:?}", style.text_styles);
        large_style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(22.0, egui::FontFamily::Proportional),
        );
        large_style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        );
        large_style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        );
        large_style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        );

        egui_extras::install_image_loaders(&cc.egui_ctx);

        // cc.egui_ctx.set_style(style);

        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        // if let Some(storage) = cc.storage {
        //     return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        // }

        let modules: HashMap<TypeId, Module> = {
            cfg_if! {
                if #[cfg(not(target_arch = "wasm32"))] {
                    crate::modules::register_generic_modules(&interop).into_iter().chain(
                        crate::modules::register_native_modules(&interop)
                    ).collect()
                } else {
                    crate::modules::register_generic_modules(&interop)
                }
            }
        };

        let mut module = if settings.developer_mode {
            modules
                .get(&TypeId::of::<modules::Testing>())
                .unwrap()
                .clone()
        } else {
            modules
                .get(&TypeId::of::<modules::Overview>())
                .unwrap()
                .clone()
        };

        if settings.version != env!("CARGO_PKG_VERSION") {
            settings.version = env!("CARGO_PKG_VERSION").to_string();
            settings.store_sync().unwrap();

            module = modules
                .get(&TypeId::of::<modules::Changelog>())
                .unwrap()
                .clone();
        }

        let channel = interop.application_events().clone();
        let wallet = interop.wallet().clone();

        let mut this = Self {
            interop,
            wallet,
            channel,
            module,
            modules: modules.clone(),
            stack: VecDeque::new(),
            settings: settings.clone(),
            toasts: Toasts::default(),

            default_style,
            large_style,

            wallet_list: Vec::new(),
            account_collection: None,
            selected_account: None,

            metrics: None,
            state: Default::default(),
            hint: None,
            discard_hint: false,
            exception: None,
        };

        modules.values().for_each(|module| {
            module.init(&mut this);
        });

        this.update_wallet_list();

        this
    }

    pub fn select<T>(&mut self)
    where
        T: 'static,
    {
        let module = self
            .modules
            .get(&TypeId::of::<T>())
            .expect("Unknown module");

        if self.module.type_id() != module.type_id() {
            self.stack.push_back(self.module.clone());
            // let previous = self.module.clone();
            self.module = module.clone();
            // previous.deactivate(self);
            // module.activate(self);

            #[cfg(not(target_arch = "wasm32"))]
            {
                let type_id = self.module.type_id();

                crate::runtime::kaspa::update_logs_flag()
                    .store(type_id == TypeId::of::<modules::Logs>(), Ordering::Relaxed);
                // crate::runtime::kaspa::update_metrics_flag().store(
                //     type_id == TypeId::of::<modules::Overview>()
                //         || type_id == TypeId::of::<modules::Metrics>()
                //         || type_id == TypeId::of::<modules::Node>(),
                //     Ordering::Relaxed,
                // );
            }
        }
    }

    pub fn has_stack(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn back(&mut self) {
        if let Some(module) = self.stack.pop_back() {
            self.module = module;
        }
    }

    pub fn sender(&self) -> crate::channel::Sender<Events> {
        self.channel.sender.clone()
    }

    pub fn wallet(&self) -> &Arc<dyn WalletApi> {
        &self.wallet
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn wallet_list(&self) -> &Vec<WalletDescriptor> {
        &self.wallet_list
    }

    pub fn account_collection(&self) -> &Option<AccountCollection> {
        &self.account_collection
    }

    pub fn modules(&self) -> &HashMap<TypeId, Module> {
        &self.modules
    }

    pub fn get<T>(&self) -> Ref<'_, T>
    where
        T: ModuleT + 'static,
    {
        let cell = self.modules.get(&TypeId::of::<T>()).unwrap();
        Ref::map(cell.inner.module.borrow(), |r| {
            (r).as_any()
                .downcast_ref::<T>()
                .expect("unable to downcast section")
        })
    }

    pub fn get_mut<T>(&mut self) -> RefMut<'_, T>
    where
        T: ModuleT + 'static,
    {
        let cell = self.modules.get_mut(&TypeId::of::<T>()).unwrap();
        RefMut::map(cell.inner.module.borrow_mut(), |r| {
            (r).as_any_mut()
                .downcast_mut::<T>()
                .expect("unable to downcast_mut module")
        })
    }
}

impl eframe::App for Core {
    #[cfg(not(target_arch = "wasm32"))]
    fn on_close_event(&mut self) -> bool {
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let handle = tokio::spawn(async move { crate::interop::interop().shutdown().await });

        while !handle.is_finished() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // println!("update...");
        for event in self.channel.iter() {
            if let Err(err) = self.handle_events(event.clone(), ctx, frame) {
                log_error!("error processing wallet interop event: {}", err);
            }
        }

        // ctx.set_visuals(self.default_style.clone());
        let mut current_visuals = ctx.style().visuals.clone(); //.widgets.noninteractive;
        let mut visuals = current_visuals.clone();
        // visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(0, 0, 0));
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(0, 0, 0);

        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                visuals.interact_cursor = Some(CursorIcon::PointingHand);
            }
        }

        // visuals.bg_fill = egui::Color32::from_rgb(0, 0, 0);
        ctx.set_visuals(visuals);
        self.toasts.show(ctx);

        theme().apply(&mut current_visuals);
        ctx.set_visuals(current_visuals);

        #[cfg(not(target_arch = "wasm32"))]
        if !self.settings.initialized {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.modules
                    .get(&TypeId::of::<modules::Welcome>())
                    .unwrap()
                    .clone()
                    .render(self, ctx, frame, ui);
            });

            return;
        }

        // if self.settings.version != env!("CARGO_PKG_VERSION") {
        //     self.settings.version = env!("CARGO_PKG_VERSION").to_string();
        //     self.settings.store_sync().unwrap();

        // }

        // let section = self.sections.get(&TypeId::of::<section::Open>()).unwrap().clone();
        // section.borrow_mut().render(self, ctx, frame, ui);
        // return;

        // let mut style = (*ctx.style()).clone();
        // // println!("style: {:?}", style.text_styles);
        // style.text_styles.insert(
        //     egui::TextStyle::Body,
        //     egui::FontId::new(18.0, egui::FontFamily::Proportional),
        // );
        // style.text_styles.insert(
        //     egui::TextStyle::Button,
        //     egui::FontId::new(18.0, egui::FontFamily::Proportional),
        // );
        // style.text_styles.insert(
        //     egui::TextStyle::Monospace,
        //     egui::FontId::new(18.0, egui::FontFamily::Proportional),
        // );

        // if crate::prompt::prompt().render(ctx) {
        //     return;
        // }

        // if let Some(wizard) = crate::stages::stages() {
        //     if wizard.render_with_context(ctx) {
        //         return;
        //     }
        // }

        // let rect = ctx.screen_rect();
        let size = ctx.screen_rect().size();

        egui::TopBottomPanel::top("top_panel").show(ctx, |_ui| {
            // The top panel is often a good place for a menu bar:
            // #[cfg(not(target_arch = "wasm32"))] // no File->Quit on web pages!
            egui::menu::bar(_ui, |ui| {
                ui.columns(2, |cols| {
                    cols[0].horizontal(|ui| {
                        ui.menu_button("File", |ui| {
                            #[cfg(not(target_arch = "wasm32"))]
                            if ui.button("Quit").clicked() {
                                frame.close();
                            }
                            ui.separator();
                            ui.label(" ~ Debug Modules ~");
                            ui.label(" ");

                            // let mut modules = self.modules.values().cloned().collect::<Vec<_>>();

                            let (tests, mut modules): (Vec<_>, Vec<_>) = self
                                .modules
                                .values()
                                .cloned()
                                .partition(|module| module.name().starts_with('~'));

                            tests.into_iter().for_each(|module| {
                                if ui.button(module.name()).clicked() {
                                    self.module = module; //.type_id();
                                    ui.close_menu();
                                }
                            });

                            ui.label(" ");

                            modules.sort_by(|a, b| a.name().partial_cmp(b.name()).unwrap());
                            modules.into_iter().for_each(|module| {
                                // let SectionInner { name,type_id, .. } = section.inner;
                                if ui.button(module.name()).clicked() {
                                    self.module = module; //.type_id();
                                    ui.close_menu();
                                }
                            });
                        });

                        ui.separator();
                        if ui.button("Overview").clicked() {
                            self.select::<modules::Overview>();
                        }
                        ui.separator();
                        if ui.button("Wallet").clicked() {
                            if self.state().is_open() {
                                self.select::<modules::AccountManager>();
                            } else {
                                self.select::<modules::WalletOpen>();
                            }
                        }
                        ui.separator();
                        // if ui.button(icon_with_text(ui, egui_phosphor::light::GEAR, Color32::WHITE, "Settings")).clicked() {
                        //     self.select::<modules::Settings>();
                        // }
                        // ui.separator();
                        // if ui.button(RichText::new(format!("{} Settings",egui_phosphor::light::GEAR))).clicked() {
                        if ui.button("Settings").clicked() {
                            self.select::<modules::Settings>();
                        }
                        cfg_if! {
                            if #[cfg(not(target_arch = "wasm32"))] {
                                ui.separator();
                                if ui.button("Node").clicked() {
                                    self.select::<modules::Node>();
                                }
                                ui.separator();
                                if ui.button("Metrics").clicked() {
                                    self.select::<modules::Metrics>();
                                }
                                ui.separator();
                                if ui.button("Logs").clicked() {
                                    self.select::<modules::Logs>();
                                }
                            }
                        }
                        ui.separator();
                        if ui.button("About").clicked() {
                            self.select::<modules::About>();
                        }
                        ui.separator();
                    });

                    cols[1].with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dictionary = i18n::dictionary();
                        #[allow(clippy::useless_format)]
                        ui.menu_button(format!("{}", dictionary.current_title()), |ui| {
                            dictionary
                                .enabled_languages()
                                .into_iter()
                                .for_each(|(code, lang)| {
                                    if ui.button(lang).clicked() {
                                        self.settings.language_code = code.to_string();
                                        dictionary
                                            .activate_language_code(code)
                                            .expect("Unable to activate language");
                                        ui.close_menu();
                                    }
                                });
                        });

                        ui.separator();

                        // let theme = theme();

                        // let icon_size = theme.panel_icon_size();
                        let icon = CompositeIcon::new(egui_phosphor::bold::MOON).icon_size(18.);
                        // .padding(Some(icon_padding));
                        // if ui.add_enabled(true, icon).clicked() {
                        if ui.add(icon).clicked() {
                            // close(self.this);
                        }

                        // if ui.button("Theme").clicked() {
                        //     self.select::<modules::Logs>();
                        // }
                        ui.separator();
                    });
                });
            });
            // ui.spacing()
        });
        // });

        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            self.render_status(ui);
            egui::warn_if_debug_build(ui);
        });
        /*
        if size.x > 600. {
            egui::SidePanel::left("left_panel").show(&ctx, |ui| {

                if ui.add(egui::Button::new("Overview")).clicked() {
                    // return Stage::Next;
                }
                if ui.add(egui::Button::new("Transactions")).clicked() {
                    // return Stage::Next;
                }

                // let section = self.sections.get(&self.section).unwrap().clone();
                // section.borrow_mut().render(self, ctx, frame, ui);
            });
        }
        */

        egui::CentralPanel::default().show(ctx, |ui| {
            // ui.style_mut().text_styles = self.large_style.text_styles.clone();

            // if false && !self.wallet().is_open() {
            if FORCE_WALLET_OPEN && !self.state.is_open() {
                //self.wallet().is_open() {
                let module = if self.module.type_id() == TypeId::of::<modules::WalletOpen>()
                    || self.module.type_id() == TypeId::of::<modules::WalletCreate>()
                {
                    self.module.clone()
                } else {
                    self.modules
                        .get(&TypeId::of::<modules::WalletOpen>())
                        .unwrap()
                        .clone()
                    // self.modules.get::<modules::WalletOpen>().unwrap().clone()
                    // TypeId::of::<modules::WalletOpen>()
                };

                // let section = match self.section {
                //      | TypeId::of::<section::Create>() => {
                //         self.section
                //     },
                //     _ => {
                //         self.sections.get(&TypeId::of::<section::Open>()).unwrap().clone()
                //     }
                // };

                // let section = self.sections.get(&section).unwrap().section.clone();
                // section.borrow_mut().render(self, ctx, frame, ui);
                // self.modules
                //     .get(&module)
                //     .unwrap()
                //     .clone()
                module.render(self, ctx, frame, ui);
            } else if ENABLE_DUAL_PANE && size.x > 500. {
                // ui.columns(2, |uis| {
                //     self.modules
                //         .get(&TypeId::of::<modules::AccountManager>())
                //         .unwrap()
                //         .clone()
                //         .render(self, ctx, frame, &mut uis[0]);
                //     self.modules.get(&self.module).unwrap().clone().render(
                //         self,
                //         ctx,
                //         frame,
                //         &mut uis[1],
                //     );
                // });
            } else {
                // self.modules
                //     .get(&self.module)
                //     .unwrap()
                //     .clone()
                self.module.clone().render(self, ctx, frame, ui);
            }
        });

        // egui::CentralPanel::default().show(ctx, |ui| {
        //     // ui.style_mut().text_styles = style.text_styles;
        //     let section = self.sections.get(&self.section).unwrap().clone();
        //     section.borrow_mut().render(self, ctx, frame, ui);
        // });

        /*
                egui::Window::new("main")
                .resize(|r|{
                    // r.resizable(false)
                    // r.fixed_size(rect.size())
                    r.fixed_size(ctx.screen_rect().size())
                })
                // .interactable(false)
                .resizable(false)
                .movable(false)
                .title_bar(false)
                .frame(egui::Frame::none())
                .show(ctx, |ui| {


                    egui::TopBottomPanel::top("top_panel").show_inside(ui, |_ui| {
                        // The top panel is often a good place for a menu bar:
                        #[cfg(not(target_arch = "wasm32"))] // no File->Quit on web pages!
                        egui::menu::bar(_ui, |ui| {
                            ui.menu_button("File", |ui| {
                                if ui.button("Quit").clicked() {
                                    frame.close();
                                }
                            });
                        });
                    });

                    egui::TopBottomPanel::bottom("bottom_panel").show_inside(ui, |ui| {
                        self.render_status(ui);
                        egui::warn_if_debug_build(ui);
                    });

                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        ui.style_mut().text_styles = style.text_styles;

                        let section = self.sections.get(&self.section).unwrap().clone();
                        section.borrow_mut().render(self, ctx, frame, ui);
                    });
                });
        */

        // if false {
        //     egui::Window::new("Window").show(ctx, |ui| {
        //         ui.label("Windows can be moved by dragging them.");
        //         ui.label("They are automatically sized based on contents.");
        //         ui.label("You can turn on resizing and scrolling if you like.");
        //         ui.label("You would normally choose either panels OR windows.");
        //     });
        // }
    }
}

impl Core {
    fn render_status(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            // ui.horizontal(|ui| {
            if !self.state().is_connected() {
                self.render_connected_state(ui, Status::Disconnected);
            } else {
                // let metrics = self.interop.kaspa_service().metrics();
                let peers = self
                    .interop
                    .kaspa_service()
                    .metrics()
                    .connected_peer_info()
                    .map(|peers| peers.len());
                let tps = self.metrics.as_ref().map(|metrics| metrics.tps);
                ui.horizontal(|ui| {
                    if self.state().is_synced() {
                        self.render_connected_state(
                            ui,
                            Status::Connected {
                                current_daa_score: self.state().current_daa_score(),
                                peers,
                                tps,
                            },
                        );
                    } else {
                        self.render_connected_state(
                            ui,
                            Status::Syncing {
                                sync_status: self
                                    .state()
                                    .sync_state
                                    .as_ref()
                                    .map(SyncStatus::try_from),
                                peers,
                            },
                        );
                    }
                });
            }
            ui.add_space(ui.available_width() - 20.);
            if icons()
                .sliders
                .render_with_options(ui, &IconSize::new(Vec2::splat(20.)), true)
                .clicked()
            {
                self.select::<modules::Settings>();
            }
        });
    }

    fn render_peers(&self, ui: &mut egui::Ui, peers: Option<usize>) {
        let status_icon_size = theme().status_icon_size;

        let peers = peers.unwrap_or(0);
        if peers != 0 {
            ui.label(format!("{} peers", peers));
        } else {
            ui.label(
                RichText::new(egui_phosphor::light::CLOUD_SLASH)
                    .size(status_icon_size)
                    .color(Color32::LIGHT_RED),
            );
            ui.label(RichText::new("No peers").color(Color32::LIGHT_RED));
        }
    }

    fn render_network_selector(&self, ui: &mut Ui) {
        ui.label(self.settings.node.network.to_string());
        // ui.menu_button(self.settings.node.network.to_string(), |ui| {
        //     Network::iter().for_each(|network| {
        //         if ui.button(network.to_string()).clicked() {
        //             ui.close_menu();
        //         }
        //     });
        // });
    }

    fn render_connected_state(&self, ui: &mut egui::Ui, state: Status) {
        //connected : bool, icon: &str, color : Color32) {
        let status_area_width = ui.available_width() - 24.;
        let status_icon_size = theme().status_icon_size;

        match state {
            Status::Disconnected => {
                ui.add_space(4.);

                match self.settings.node.node_kind {
                    KaspadNodeKind::Disable => {
                        ui.label(
                            RichText::new(egui_phosphor::light::PLUGS)
                                .size(status_icon_size)
                                .color(Color32::LIGHT_RED),
                        );
                        ui.separator();
                        ui.label("Not Connected");
                    }
                    KaspadNodeKind::Remote => {
                        ui.label(
                            RichText::new(egui_phosphor::light::TREE_STRUCTURE)
                                .size(status_icon_size)
                                .color(Color32::LIGHT_RED),
                        );
                        ui.separator();
                        ui.label("Connecting...");
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    _ => {
                        ui.label(
                            RichText::new(egui_phosphor::light::PLUGS)
                                .size(status_icon_size)
                                .color(Color32::LIGHT_RED),
                        );
                        ui.separator();
                        ui.label("Starting...");
                    }
                }
                // if self.settings.node.node_kind != KaspadNodeKind::Disable {
                //     ui.label("Not Connected");

                // } else {
                //     ui.label("Not Connected");
                // }
            }

            Status::Connected {
                current_daa_score,
                peers,
                tps: _,
            } => {
                ui.add_space(4.);
                ui.label(
                    RichText::new(egui_phosphor::light::CPU)
                        .size(status_icon_size)
                        .color(Color32::LIGHT_GREEN),
                );
                ui.separator();
                // if peers.unwrap_or(0) != 0 {
                // ui.label("ONLINE");
                // } else {
                ui.label("CONNECTED");
                // }
                ui.separator();
                self.render_network_selector(ui);
                // ui.menu_button(self.settings.node.network.to_string(), |ui| {
                //     Network::iter().for_each(|network| {
                //         if ui.button(network.to_string()).clicked() {
                //             ui.close_menu();
                //         }
                //     });
                // });

                ui.separator();
                self.render_peers(ui, peers);
                if let Some(current_daa_score) = current_daa_score {
                    ui.separator();
                    ui.label(format!("DAA {}", current_daa_score.separated_string()));
                }
            }
            Status::Syncing { sync_status, peers } => {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(4.);
                        ui.label(
                            RichText::new(egui_phosphor::light::CLOUD_ARROW_DOWN)
                                .size(status_icon_size)
                                .color(Color32::YELLOW),
                        );
                        ui.separator();
                        ui.label("CONNECTED");
                        ui.separator();
                        self.render_network_selector(ui);

                        // ui.label(self.settings.node.network.to_string());
                        ui.separator();
                        self.render_peers(ui, peers);
                        if let Some(status) = sync_status.as_ref() {
                            if !status.synced {
                                ui.separator();
                                status.render_text_state(ui);
                            }
                        }
                    });

                    if let Some(status) = sync_status.as_ref() {
                        if !status.synced {
                            status
                                .progress_bar()
                                .map(|bar| ui.add(bar.desired_width(status_area_width)));
                        }
                    }
                });
            }
        }
    }

    pub fn handle_events(
        &mut self,
        event: Events,
        _ctx: &egui::Context,
        _frame: &mut eframe::Frame,
    ) -> Result<()> {
        match event {
            Events::UpdateLogs => {}
            Events::Metrics { snapshot } => {
                self.metrics = Some(snapshot);
            }
            Events::Exit => {
                println!("Exit...");
                cfg_if! {
                    if #[cfg(not(target_arch = "wasm32"))] {
                        _frame.close();
                    }
                }
            }
            Events::Error(_error) => {}
            Events::WalletList { wallet_list } => {
                // println!("getting wallet list!, {:?}", wallet_list);
                self.wallet_list = (*wallet_list).clone();
                self.wallet_list.sort();
                // self.wallet_list.sort_by(|a, b| {
                //     // a.title.partial_cmp(&b.title).unwrap()
                //     a.filename.partial_cmp(&b.filename).unwrap()
                // });
            }
            // Events::AccountList { account_list } => {
            //     self.account_collection = Some((*account_list).into());
            //     // self.account_list = Some((*account_list).clone());
            //     // self.account_map = Some(
            //     //     (*account_list)
            //     //         .clone()
            //     //         .into_iter()
            //     //         .map(|account| (account.id(), account)).collect::<HashMap::<_,_>>()
            //     // );
            // }
            Events::Notify { notification } => {
                notification.render(&mut self.toasts);
            }
            Events::Close { .. } => {}
            // Events::Send { .. } => { },
            // Events::Deposit { .. } => { },

            // Events::TryUnlock(_secret) => {
            //     let mut unlock = wallet.get_mut::<section::Unlock>();
            //     unlock.message = Some("Error unlocking wallet...".to_string());
            //     unlock.lock();
            // },
            Events::UnlockSuccess => {
                // self.select::<section::Account>();
            }
            Events::UnlockFailure { .. } => {}
            Events::Wallet { event } => {
                match *event {
                    CoreWallet::UtxoProcStart => {

                        // println!("UtxoProcStart...");
                        // self.wallet_list();
                    }
                    CoreWallet::UtxoProcStop => {}
                    CoreWallet::UtxoProcError { message: _ } => {
                        // terrorln!(this,"{err}");
                    }
                    #[allow(unused_variables)]
                    CoreWallet::Connect { url, network_id } => {
                        // log_info!("Connected to {url}");
                        self.state.is_connected = true;
                        self.state.url = url;
                        self.state.network_id = Some(network_id);
                    }
                    #[allow(unused_variables)]
                    CoreWallet::Disconnect {
                        url: _,
                        network_id: _,
                    } => {
                        self.state.is_connected = false;
                        self.state.sync_state = None;
                        self.state.is_synced = None;
                        self.state.server_version = None;
                        self.state.url = None;
                        self.state.network_id = None;
                        self.state.current_daa_score = None;
                        self.metrics = None;
                    }
                    CoreWallet::UtxoIndexNotEnabled { url } => {
                        self.exception = Some(Exception::UtxoIndexNotEnabled { url });
                    }
                    CoreWallet::SyncState { sync_state } => {
                        self.state.sync_state = Some(sync_state);
                    }
                    CoreWallet::ServerStatus {
                        is_synced,
                        server_version,
                        url,
                        network_id,
                    } => {
                        self.state.is_synced = Some(is_synced);
                        self.state.server_version = Some(server_version);
                        self.state.url = url;
                        self.state.network_id = Some(network_id);
                    }
                    CoreWallet::WalletHint { hint } => {
                        self.hint = hint;
                        self.discard_hint = false;
                    }
                    CoreWallet::WalletOpen {
                        account_descriptors,
                    }
                    | CoreWallet::WalletReload {
                        account_descriptors,
                    } => {
                        self.state.is_open = true;

                        if let Some(account_descriptors) = account_descriptors {
                            let account_list = account_descriptors
                                .into_iter()
                                .map(Account::from)
                                .collect::<Vec<_>>();

                            self.account_collection = Some(account_list.into());
                        } else {
                            log_error!("CoreWallet::(WalletOpen | WalletReset): missing account descriptors");
                        }

                        // self.update_account_list();
                    }
                    CoreWallet::WalletError { message: _ } => {
                        // self.state.is_open = false;
                    }
                    CoreWallet::WalletReady => {}
                    CoreWallet::WalletClose => {
                        self.hint = None;
                        self.state.is_open = false;
                        self.account_collection = None;
                    }
                    CoreWallet::AccountSelection { id: _ } => {
                        // self.selected_account = self.wallet().account().ok();
                    }
                    CoreWallet::DAAScoreChange { current_daa_score } => {
                        self.state.current_daa_score.replace(current_daa_score);
                    }
                    CoreWallet::External { record } |
                    CoreWallet::Pending {
                        record, ..
                        // is_outgoing: _,
                    } |
                    CoreWallet::Maturity {
                        record, ..
                        // is_outgoing: _,
                    } |
                    CoreWallet::Outgoing { record } => {

                        match record.binding().clone() {
                            Binding::Account(id) => {

                                self.account_collection.as_ref().and_then(|account_collection| {
                                    account_collection.get(&id).map(|account| {
                                        account.transactions().replace_or_insert(record.into());
                                    })
                                });
                                // .or_else(|| {
                                //     log_error!("unable to find account {}", id);
                                //     None
                                // });

                                // if let Some(account_collection) = &self.account_collection {
                                //     if let Some(account) = account_collection.get(id) {
                                //         account.transactions().replace_or_insert(record.into());
                                //     } else {
                                //         log_error!("unable to find account {}", id);
                                //     }
                                // } else {
                                //     log_error!(
                                //         "received CoreWallet Transaction Data while account collection is empty"
                                //     );
                                // }
                            }
                            Binding::Custom(_) => {
                                panic!("custom binding not supported");
                            }
                        }

                    }

                    CoreWallet::Reorg { record } => {
                        match record.binding().clone() {
                            Binding::Account(id) => {

                                self.account_collection.as_mut().and_then(|account_collection| {
                                    account_collection.get(&id).map(|account| {
                                        account.transactions().remove(record.id())
                                    })
                                });
                                // .or_else(|| {
                                //     log_error!("unable to find account {}", id);
                                //     None
                                // });

                                // if let Some(account_collection) = &self.account_collection {
                                //     if let Some(account) = account_collection.get(id) {
                                //         account.transactions().replace_or_insert(record.into());
                                //     } else {
                                //         log_error!("unable to find account {}", id);
                                //     }
                                // } else {
                                //     log_error!(
                                //         "received CoreWallet Transaction Data while account collection is empty"
                                //     );
                                // }
                            }
                            Binding::Custom(_) => {
                                panic!("custom binding not supported");
                            }
                        }
                    }

                    CoreWallet::Balance {
                        balance,
                        id,
                        mature_utxo_size,
                        pending_utxo_size,
                    } => {
                        if let Some(account_collection) = &self.account_collection {
                            if let Some(account) = account_collection.get(&id.into()) {
                                println!("*** updating account balance: {}", id);
                                account.update_balance(
                                    balance,
                                    mature_utxo_size,
                                    pending_utxo_size,
                                )?;
                            } else {
                                log_error!("unable to find account {}", id);
                            }
                        } else {
                            log_error!(
                                "received CoreWallet::Balance while account collection is empty"
                            );
                        }
                    }
                }
            } // _ => unimplemented!()
        }

        Ok(())
    }

    pub fn update_wallet_list(&self) {
        let interop = self.interop.clone();
        spawn(async move {
            let wallet_list = interop.wallet().wallet_enumerate().await?;
            interop
                .send(Events::WalletList {
                    wallet_list: Arc::new(wallet_list),
                })
                .await?;
            Ok(())
        });
    }

    // pub fn update_account_list(&self) {
    //     let interop = self.interop.clone();
    //     spawn(async move {
    //         // let account_map = HashMap::group_from(account_list.into_iter().map(|account| (account.prv_key_data_id().unwrap(), account)));
    //         let account_list = interop.wallet().account_enumerate().await?; //activate_all_stored_accounts().await?;
    //         let account_list = account_list
    //             .into_iter()
    //             .map(Account::from)
    //             .collect::<Vec<_>>();
    //         interop
    //             .send(Events::AccountList {
    //                 account_list: Box::new(account_list),
    //             })
    //             .await?;
    //         // interop
    //         //     .wallet()
    //         //     .autoselect_default_account_if_single()
    //         //     .await
    //         //     .ok();
    //         Ok(())
    //     });
    // }
}
