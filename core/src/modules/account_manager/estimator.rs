use crate::imports::*;
use super::*;

pub struct Estimator<'context> {
    context: &'context mut ManagerContext
}

impl<'context> Estimator<'context> {
    pub fn new(context: &'context mut ManagerContext) -> Self {
        Self { context }
    }

    pub fn render(&mut self, core : &mut Core, ui: &mut Ui, rc : &RenderContext) -> bool {


        use egui_phosphor::light::{CHECK, X};

        let RenderContext { network_type, .. } = rc;
        let network_id = NetworkId::from(core.network());

        let mut request_send = false;
        let mut request_estimate = self.context.request_estimate.take().unwrap_or_default();

        match self.context.transaction_kind.as_ref().unwrap() {
            TransactionKind::Send => {
                Destination::new(self.context).render(core, ui, rc);
            }
            TransactionKind::Transfer => {
                Transfer::new(self.context).render(core, ui, rc);
            }
        }

        let response = TextEditor::new(
            &mut self.context.send_amount_text,
            &mut self.context.focus,
            Focus::Amount,
            |ui, text| {
                ui.add_space(8.);
                ui.label(RichText::new(format!("{} {} {}", i18n("Enter"), kaspa_suffix(network_type), i18n("amount to send"))).size(12.).raised());
                ui.add_sized(Overview::editor_size(ui), TextEdit::singleline(text)
                    .vertical_align(Align::Center))
            },
        )
        .change(|_| {
            request_estimate = true;
        })
        .build(ui);

        if response.text_edit_submit(ui) {
            self.context.focus.next(Focus::Fees);
        }

        ui.add_space(8.);

        TextEditor::new(
            &mut self.context.priority_fees_text,
            &mut self.context.focus,
            Focus::Fees,
            |ui, text| {
                ui.add_space(8.);
                ui.label(RichText::new("Enter priority fees").size(12.).raised());
                ui.add_sized(Overview::editor_size(ui), TextEdit::singleline(text)
                    .vertical_align(Align::Center))
            },
        )
        .change(|_| {
            request_estimate = true;
        })
        .submit(|_,_|{
            request_send = true;
        })
        .build(ui); 

        core.apply_default_style(ui);

        let (ready_to_send, actual_estimate) = match &*self.context.estimate.lock().unwrap() {
            EstimatorStatus::GeneratorSummary(actual_estimate) => {                
                let ready_to_send = self.context.address_status == AddressStatus::Valid || (self.context.transaction_kind == Some(TransactionKind::Transfer) && self.context.transfer_to_account.is_some());
                (ready_to_send, actual_estimate.clone())
            }
            EstimatorStatus::Error(error) => {
                ui.label(RichText::new(error.to_string()).color(theme_color().error_color));
                (false, GeneratorSummary::new(network_id))
            }
            EstimatorStatus::None => {
                ui.label(format!("{} {} {}", i18n("Please enter"), kaspa_suffix(network_type), i18n("amount to send")));
                (false, GeneratorSummary::new(network_id))
            }
        };

        let network_type = *network_type;
        let network_pressure = core.network_pressure.capacity();

        let usd_rate = if core.settings.market_monitor {
            core.market.as_ref().and_then(|market| {
                market.price.as_ref().and_then(|price_list| {
                    price_list.get("usd").map(|market_data| market_data.price)
                })
            })
        } else { None };

        let buckets = if let Some(fees) = core.feerate.as_ref() {
            [Some(FeeMode::Low(fees.low.value())), Some(FeeMode::Economic(fees.economic.value())), Some(FeeMode::Priority(fees.priority.value()))]
        } else { [None, None, None] };

        ui.add_space(8.);
        ui.heading("Priority Fee Estimator");

        let mut fee_selection = SelectionPanels::new(
            120.0,
            150.0);

        for mode in buckets.into_iter().flatten() {
            let bucket = mode.bucket();
            let aggregate_mass = actual_estimate.aggregate_mass;
            let number_of_generated_stages = actual_estimate.number_of_generated_stages;
            let feerate = bucket.feerate;
            let seconds = bucket.seconds.max(1.0) * number_of_generated_stages as f64;
            let total_kas = feerate * aggregate_mass as f64 * 1e-8;
            let total_sompi = (feerate * aggregate_mass as f64) as u64;
            let total_usd = usd_rate.map(|rate| total_kas * rate);
            fee_selection = fee_selection.add_with_footer(mode, i18n(mode.to_string().as_str()), format_duration_estimate(seconds), move |ui| {
                ui.label(RichText::new(sompi_to_kaspa_string_with_suffix(total_sompi, &network_type)).strong());
                if let Some(usd) = total_usd {
                    let usd = format_currency(usd, 6);
                    ui.label(RichText::new(format!("~{} USD", usd)).strong());
                }
                ui.label(format!("{} SOMPI/g", format_with_precision(feerate)));
            });
        }

        // if fee_selection.render(ui, &mut self.context.fee_mode).clicked() {
        let mode = self.context.fee_mode;
        fee_selection.render(ui, &mut self.context.fee_mode);
        if mode != self.context.fee_mode {
            let bucket = self.context.fee_mode.bucket();
            let priority_feerate = (bucket.feerate - 1.0).max(0.0);
            let total_fees_sompi = (priority_feerate * actual_estimate.aggregate_mass as f64) as u64;
            // runtime().toast(UserNotification::success(format!("selection: {:?}", self.context.fee_mode)).short());
            let total_fee_kaspa = sompi_to_kaspa(total_fees_sompi);
            self.context.priority_fees_text = if total_fee_kaspa < 0.0001 {
                format!("{}", total_fee_kaspa)
            } else if total_fee_kaspa < 0.01 {
                format!("{:0.6}", total_fee_kaspa)
            } else {
                format!("{:0.4}", sompi_to_kaspa(total_fees_sompi))
            };
            self.context.fee_mode = FeeMode::None;
            request_estimate = true;
        }


        ui.vertical_centered(|ui| {

            ui.label(format!("{} {}  •  {} {}  •  {} {}g  •  {} ~{}%",
                i18n("Transactions:"), 
                actual_estimate.number_of_generated_transactions, 
                i18n("UTXOs:"),
                actual_estimate.aggregated_utxos,
                i18n("Mass:"),
                actual_estimate.aggregate_mass,
                i18n("Network Load:"),
                network_pressure, 
            ));

            ui.add_space(8.);

            if let Some(final_transaction_amount) = actual_estimate.final_transaction_amount {
                ui.heading(RichText::new(format!("{} {}",i18n("Final Amount:"), sompi_to_kaspa_string_with_suffix(final_transaction_amount + actual_estimate.aggregate_fees, &network_type))).strong());
            }

        });

        ui.add_space(16.);

        core.apply_mobile_style(ui);

        if request_send {
            if ready_to_send {
                self.context.action = Action::Sending;
                self.context.focus.next(Focus::WalletSecret);
            } else if self.context.address_status != AddressStatus::Valid {
                self.context.focus.next(Focus::Address);
            }
        }

        ui.horizontal(|ui| {
            ui.vertical_centered(|ui|{
                ui.horizontal(|ui| {
                    CenterLayoutBuilder::new()
                        .add_enabled(ready_to_send, Button::new(format!("{CHECK} {}", i18n("Send"))).min_size(theme_style().medium_button_size()), |this: &mut Estimator<'_>| {
                            this.context.action = Action::Sending;
                            this.context.focus.next(Focus::WalletSecret);
                        })
                        .add(Button::new(format!("{X} {}", i18n("Cancel"))).min_size(theme_style().medium_button_size()), |this| {
                            this.context.reset_send_state();
                        })
                        .build(ui, self)
                });
            });

        });

        ui.add_space(16.);

        self.update_user_args() 
            && request_estimate 
            && matches!(self.context.action,Action::Estimating)

    }



    fn update_user_args(&mut self) -> bool {
        let mut valid = true;

        match try_kaspa_str_to_sompi(self.context.send_amount_text.as_str()) {
            Ok(Some(sompi)) => {
                self.context.send_amount_sompi = sompi;
            }
            Ok(None) => {
                self.user_error(i18n("Please enter an amount").to_string());
                valid = false;
            }
            Err(err) => {
                self.user_error(format!("{} {err}", i18n("Invalid amount:")));
                valid = false;
            }
        }

        match try_kaspa_str_to_sompi(self.context.priority_fees_text.as_str()) {
            Ok(Some(sompi)) => {
                self.context.priority_fees_sompi = sompi;
            }
            Ok(None) => {
                self.context.priority_fees_sompi = 0;
            }
            Err(err) => {
                self.user_error(format!("{} {err}", i18n("Invalid fee amount:")));
                valid = false;
            }
        }

        valid
    }

    fn user_error(&self, error : impl Into<String>) {
        *self.context.estimate.lock().unwrap() = EstimatorStatus::Error(error.into());
    }
        
}


fn format_duration_estimate(seconds: f64) -> String {
    let minutes = (seconds / 60.0) as u64;
    let seconds = seconds as u64;

    if seconds == 1 {
        format!("< {seconds} second")
    } else if seconds < 60 {
        format!("< {seconds} seconds")
    } else if minutes == 1 {
        format!("< {minutes} minute")
    } else {
        format!("< {minutes} minutes")
    }
}