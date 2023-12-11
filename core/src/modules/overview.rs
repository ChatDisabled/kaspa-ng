use std::borrow::Cow;

use egui::load::Bytes;
use kaspa_metrics_core::{Metric,MetricGroup};
use egui_plot::{
    Legend,
    Line,
    LineStyle,
    Plot,
    PlotPoints,
};

use crate::imports::*;

pub struct Overview {
    #[allow(dead_code)]
    runtime: Runtime,
}

impl Overview {
    pub fn new(runtime: Runtime) -> Self {
        Self { runtime }
    }
}

impl ModuleT for Overview {

    fn style(&self) -> ModuleStyle {
        ModuleStyle::Default
    }


    fn render(
        &mut self,
        core: &mut Core,
        _ctx: &egui::Context,
        _frame: &mut eframe::Frame,
        ui: &mut egui::Ui,
    ) {
        let width = ui.available_width();

        if core.device().single_pane() {
            self.render_details(core, ui);
        } else {
            SidePanel::left("overview_left").exact_width(width/2.).resizable(false).show_separator_line(true).show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_source("overview_metrics")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        self.render_stats(core,ui);
                    });
            });

            SidePanel::right("overview_right")
                .exact_width(width/2.)
                .resizable(false)
                .show_separator_line(false)
                .show_inside(ui, |ui| {
                    self.render_details(core, ui);
                });
        }


    }
}

impl Overview {

    fn render_stats(&mut self, core: &mut Core, ui : &mut Ui) {

        CollapsingHeader::new(i18n("Kaspa p2p Node"))
        .default_open(true)
        .show(ui, |ui| {

            if core.state().is_connected() {
                self.render_graphs(core,ui);
            } else {
                ui.label(i18n("Not connected"));
            }
        });

        ui.add_space(48.);
    }

    fn render_details(&mut self, core: &mut Core, ui : &mut Ui) {

        let screen_rect = ui.ctx().screen_rect();
        let logo_size = vec2(648., 994.,) * 0.25;
        let left = screen_rect.width() - logo_size.x - 8.;
        let top = 32.;
        let logo_rect = Rect::from_min_size(Pos2::new(left, top), logo_size);

        if screen_rect.width() > 768.0 {
            Image::new(ImageSource::Bytes { uri : Cow::Borrowed("bytes://logo.svg"), bytes : Bytes::Static(crate::app::KASPA_NG_LOGO_SVG)})
            .maintain_aspect_ratio(true)
            .max_size(logo_size)
            .fit_to_exact_size(logo_size)
            .shrink_to_fit()
            .texture_options(TextureOptions::LINEAR)
            .tint(Color32::from_f32(0.8))
            .paint_at(ui, logo_rect);
        }

    egui::ScrollArea::vertical()
        .id_source("overview_metrics")
        .auto_shrink([false; 2])
        .show(ui, |ui| {

            CollapsingHeader::new(i18n("Market"))
                .default_open(true)
                .show(ui, |ui| {

                    if let Some(price_list) = core.market.price.as_ref() {
                        for (symbol, data) in price_list.iter() {
                            if let Some(price) = data.price {
                                ui.label(RichText::new(format!("{} {}  ",price,symbol.to_uppercase())));//.font(FontId::proportional(14.)));
                            }
                        }
                    }

                });

            CollapsingHeader::new(i18n("Resources"))
                .default_open(true)
                .show(ui, |ui| {
                    // egui::special_emojis
                    // use egui_phosphor::light::{DISCORD_LOGO,GITHUB_LOGO};
                    ui.hyperlink_to_tab(
                        format!("• {}",i18n("Kaspa NG on GitHub")),
                        "https://github.com/aspectron/kaspa-ng"
                    );
                    ui.hyperlink_to_tab(
                        format!("• {}",i18n("Rusty Kaspa on GitHub")),
                        "https://github.com/kaspanet/rusty-kaspa",
                    );
                    ui.hyperlink_to_tab(
                        format!("• {}",i18n("NPM Modules for NodeJS")),
                        "https://www.npmjs.com/package/kaspa",
                    );
                    ui.hyperlink_to_tab(
                        format!("• {}",i18n("WASM SDK for JavaScript and TypeScript")),
                        "https://github.com/kaspanet/rusty-kaspa/wasm",
                    );
                    ui.hyperlink_to_tab(
                        format!("• {}",i18n("Rust Wallet SDK")),
                        "https://docs.rs/kaspa-wallet-core/0.0.4/kaspa_wallet_core/",
                    );
                    ui.hyperlink_to_tab(
                        format!("• {}",i18n("Kaspa Discord")),
                        "https://discord.com/invite/kS3SK5F36R",
                    );
                });

            if let Some(release) = core.release.as_ref() {
                if release.version == crate::app::VERSION {
                    CollapsingHeader::new(i18n("Redistributables"))
                        .id_source("redistributables")
                        .default_open(false)
                        .show(ui, |ui| {
                            release.assets.iter().for_each(|asset| {
                                Hyperlink::from_label_and_url(
                                    format!("• {}", asset.name),
                                    asset.browser_download_url.clone(),
                                ).open_in_new_tab(true).ui(ui);
                            });
                        });
                } else {
                    CollapsingHeader::new(RichText::new(format!("{} {}",i18n("Update Available to version"), release.version)).color(theme_color().alert_color).strong())
                        .id_source("redistributables-update")
                        .default_open(true)
                        .show(ui, |ui| {

                            if let Some(html_url) = &release.html_url {
                                Hyperlink::from_label_and_url(
                                    format!("• {} {}", i18n("GitHub Release"), release.version),
                                    html_url,
                                ).open_in_new_tab(true).ui(ui);
                            }

                            release.assets.iter().for_each(|asset| {
                                Hyperlink::from_label_and_url(
                                    format!("• {}", asset.name),
                                    asset.browser_download_url.clone(),
                                ).open_in_new_tab(true).ui(ui);
                            });

                        });

                }
            }

            CollapsingHeader::new(i18n("Build"))
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(format!("Kaspa NG v{}-{} + Rusty Kaspa v{}", env!("CARGO_PKG_VERSION"),crate::app::GIT_DESCRIBE, kaspa_wallet_core::version()));
                    ui.label(format!("Timestamp: {}", crate::app::BUILD_TIMESTAMP));
                    ui.label(format!("rustc {}-{} {}  llvm {}", 
                        crate::app::RUSTC_SEMVER,
                        crate::app::RUSTC_COMMIT_HASH.chars().take(8).collect::<String>(),
                        crate::app::RUSTC_CHANNEL,
                        crate::app::RUSTC_LLVM_VERSION,
                    ));
                    ui.label(format!("architecture {}", 
                        crate::app::CARGO_TARGET_TRIPLE
                    ));
                });

            if let Some(system) = runtime().system() {
                system.render(ui);
            }
    
            CollapsingHeader::new(i18n("License Information"))
                .default_open(false)
                .show(ui, |ui| {
                    ui.vertical(|ui|{
                        ui.label("Rusty Kaspa");
                        ui.label("Copyright (c) 2023 Kaspa Developers");
                        ui.label("License: ISC");
                        ui.hyperlink_url_to_tab("https://github.com/kaspanet/rusty-kaspa");
                        ui.label("");
                        ui.label("Kaspa NG");
                        ui.label("Copyright (c) 2023 ASPECTRON");
                        ui.label("License: MIT or Apache 2.0");
                        ui.hyperlink_url_to_tab("https://github.com/aspectron/kaspa-ng");
                        ui.label("");
                        ui.label("WORKFLOW-RS");
                        ui.label("Copyright (c) 2023 ASPECTRON");
                        ui.label("License: MIT");
                        ui.hyperlink_url_to_tab("https://github.com/workflow-rs/workflow-rs");
                        ui.label("");
                        ui.label("EGUI");
                        ui.label("Copyright (c) 2023 Rerun");
                        ui.label("License: MIT or Apache 2.0");
                        ui.hyperlink_url_to_tab("https://github.com/emilk/egui");
                        ui.label("");
                        ui.label("PHOSPHOR ICONS");
                        ui.label("Copyright (c) 2023 ");
                        ui.label("License: MIT");
                        ui.hyperlink_url_to_tab("https://phosphoricons.com/");
                        ui.label("");
                        ui.label("Illustration Art");
                        ui.label("Copyright (c) 2023 Rhubarb Media");
                        ui.label("License: CC BY 4.0");
                        ui.hyperlink_url_to_tab("https://rhubarbmedia.ca/");
                        ui.label("");
                    });
                });

                CollapsingHeader::new(i18n("Credits"))
                .default_open(false)
                .show(ui, |ui| {
                    ui.vertical(|ui|{
                        ui.set_width(ui.available_width() - 48.);
                        ui.label("Special thanks Kaspa developers and the following community members:");
                        ui.horizontal_wrapped(|ui|{
                            let nicks = [
                                "0xAndrei",
                                "142673",
                                "Bape",
                                "Bubblegum Lightning",
                                "coderofstuff",
                                "CryptoK",
                                "Dhayse",
                                "elertan0",
                                "elichai2",
                                "Gennady Gorin",
                                "hashdag",
                                "Helix",
                                "jablonx",
                                "jwj",
                                "KaffinPX",
                                "lAmeR",
                                "matoo",
                                "msutton",
                                "n15a",
                                "Rhubarbarian",
                                "shaideshe",
                                "someone235",
                                "supertypo",
                                "The AllFather",
                                "Tim",
                                "tmrlvi",
                                "Wolfie",
                            ];

                            let text = nicks.into_iter().map(|nick|format!("@{nick}  ")).collect::<Vec<_>>().join(" ");
                            ui.label(text);
                        });
                    });
                });

                CollapsingHeader::new(i18n("Donations"))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label("Please support Kaspa NG development");
                        // if ui.link("kaspatest:qqdr2mv4vkes6kvhgy8elsxhvzwde42629vnpcxe4f802346rnfkklrhz0x7x").clicked() {
                        let donation_address = "kaspatest:qqdr2mv4vkes6kvhgy8elsxhvzwde42629vnpcxe4f802346rnfkklrhz0x7x";
                        if ui.link(format_address(&Address::try_from(donation_address).unwrap(), Some(12))).clicked() {
                            println!("link clicked...");
                        }
                    });
        });
    }

    fn render_graphs(&mut self, core: &mut Core, ui : &mut Ui) {

        let mut metric_iter = METRICS.iter();

        if let Some(snapshot) = core.metrics.as_ref() {
            let view_width = ui.available_width();
            if view_width < 200. {
                return;
            }
            let graph_columns = ((view_width-48.) / 128.) as usize;

            let mut draw = true;
            while draw {
                ui.horizontal(|ui| {
                    for _ in 0..graph_columns {
                        if let Some(metric) = metric_iter.next() {
                            let value = snapshot.get(metric);
                            self.render_graph(ui,  *metric, value);
                        } else {
                            draw = false;
                        }
                    }
                });
            }
        }

    }

    fn render_graph(&mut self, ui : &mut Ui, metric : Metric, value : f64) {

        let group = MetricGroup::from(metric);
        let graph_color = group.to_color();

        let graph_data = {
            let metrics_data = self.runtime.metrics_service().metrics_data();
            let data = metrics_data.get(&metric).unwrap();
            let mut duration = 2 * 60;
            let available_samples = runtime().metrics_service().samples_since_connection();
            if available_samples < duration {
                duration = available_samples;
            }
            let samples = if data.len() < duration { data.len() } else { duration };
            data[data.len()-samples..].to_vec()
        };

        
        ui.vertical(|ui|{
            let frame = 
            Frame::none()
                // .fill(Color32::from_rgb(240,240,240))
                .stroke(Stroke::new(1.0, theme_color().graph_frame_color))
                // .inner_margin(4.)
                .inner_margin(Margin { left: 3., right: 3., top: 4., bottom: 4. })
                .outer_margin(8.)
                // .rounding(8.)
                .rounding(6.);

            frame.show(ui, |ui| {

                let mut plot = Plot::new(metric.as_str())
                    .legend(Legend::default())
                    .width(128.)
                    .height(32.)
                    .auto_bounds_x()
                    .auto_bounds_y()
                    .set_margin_fraction(vec2(0.0,0.0) )
                    .show_axes(false)
                    .show_grid(false)
                    .allow_drag([false, false])
                    .allow_scroll(false)
                    .show_background(false)
                    .show_x(false)
                    .show_y(false)
                    ;

                if [Metric::NodeCpuUsage].contains(&metric) {
                    plot = plot.include_y(100.);
                }

                // let color = graph_color.gamma_multiply(0.5);
                let line = Line::new(PlotPoints::Owned(graph_data))
                    // .color(color)
                    .color(graph_color)
                    .style(LineStyle::Solid)
                    .fill(0.0);

                let plot_result = plot.show(ui, |plot_ui| {
                    plot_ui.line(line);
                });

                let text = format!("{} {}", i18n(metric.title().1).to_uppercase(), metric.format(value, true, true));
                let rich_text_top = RichText::new(&text).size(10.).color(theme_color().raised_text_color);
                let rich_text_back = RichText::new(text).size(10.).color(theme_color().raised_text_shadow);
                let label_top = Label::new(rich_text_top).wrap(false);
                let label_back = Label::new(rich_text_back).wrap(false);
                let mut rect_top = plot_result.response.rect;
                rect_top.set_bottom(rect_top.top() + 12.);
                let mut rect_back = rect_top;
                rect_back.set_center(rect_back.center()+vec2(0.8,0.8));
                ui.put(rect_back, label_back);
                ui.put(rect_top, label_top);
            });
        });
    }
}

const METRICS : &[Metric] = &[
    Metric::NodeCpuUsage,
    Metric::NodeResidentSetSizeBytes,
    // Metric::VirtualMemorySizeBytes,
    Metric::NodeFileHandlesCount,
    Metric::NodeDiskIoReadBytes,
    Metric::NodeDiskIoReadPerSec,
    Metric::NodeDiskIoWriteBytes,
    Metric::NodeDiskIoWritePerSec,
    // Metric::BorshLiveConnections,
    // Metric::BorshConnectionAttempts,
    // Metric::BorshHandshakeFailures,
    // Metric::JsonLiveConnections,
    // Metric::JsonConnectionAttempts,
    // Metric::JsonHandshakeFailures,
    Metric::NodeTotalBytesRx,
    Metric::NodeTotalBytesRxPerSecond,
    Metric::NodeTotalBytesTx,
    Metric::NodeTotalBytesTxPerSecond,
    Metric::NodeActivePeers,
    Metric::NodeBlocksSubmittedCount,
    Metric::NodeHeadersProcessedCount,
    Metric::NodeDependenciesProcessedCount,
    Metric::NodeBodiesProcessedCount,
    Metric::NodeTransactionsProcessedCount,
    Metric::NodeChainBlocksProcessedCount,
    Metric::NodeMassProcessedCount,
    Metric::NodeDatabaseBlocksCount,
    Metric::NodeDatabaseHeadersCount,
    Metric::NetworkMempoolSize,
    Metric::NetworkTransactionsPerSecond,
    Metric::NetworkTipHashesCount,
    Metric::NetworkDifficulty,
    Metric::NetworkPastMedianTime,
    Metric::NetworkVirtualParentHashesCount,
    Metric::NetworkVirtualDaaScore,
];