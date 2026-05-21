use anyhow::Result;
use async_trait::async_trait;
use teloxide::{
    Bot,
    payloads::SendMessageSetters,
    prelude::Requester,
    sugar::request::RequestLinkPreviewExt,
    types::{ChatId, ParseMode, Recipient},
};

use crate::{
    application::Notifier,
    domain::{JupiterSwapEvent, PumpFunTrade, RaydiumSwapEvent, SwapEvent},
};

pub struct TelegramNotifier {
    bot: Bot,
    chat_id: Recipient,
}

impl TelegramNotifier {
    pub fn new(token: String, chat_id_str: String) -> Self {
        let bot = Bot::new(token);
        let chat_id = match chat_id_str.parse::<i64>() {
            Ok(id) => Recipient::Id(ChatId(id)),
            Err(_) => Recipient::ChannelUsername(chat_id_str),
        };
        Self { bot, chat_id }
    }

    fn short(&self, key: &str) -> String {
        key.chars().take(8).collect()
    }

    fn fmt_raydium(&self, s: &RaydiumSwapEvent) -> String {
        format!(
            "🚨 <b>Whale Swap (Raydium)</b>\n\n\
            <b>Pool:</b> <code>{}</code>\n\
            <b>In:</b> {} <code>{}</code>\n\
            <b>Out:</b> {} <code>{}</code>\n\
            <b>Signer:</b> <a href=\"https://solscan.io/account/{}\">{}</a>\n\n\
            <a href=\"https://solscan.io/tx/{}\">View tx</a>",
            s.amm_pool,
            s.amount_in, s.mint_source,
            s.amount_received, s.mint_destination,
            s.signer, self.short(&s.signer),
            s.signature,
        )
    }

    fn fmt_jupiter(&self, s: &JupiterSwapEvent) -> String {
        let route = if s.route_plan.is_empty() {
            "-".to_string()
        } else {
            s.route_plan.iter().map(|r| format!("{} ({}%)", r.swap_label, r.percent)).collect::<Vec<_>>().join(" → ")
        };
        format!(
            "🚨 <b>Whale Swap (Jupiter)</b>\n\n\
            <b>Pool:</b> <code>{}</code>\n\
            <b>In:</b> {} <code>{}</code>\n\
            <b>Out:</b> {} <code>{}</code>\n\
            <b>Route:</b> {}\n\
            <b>Signer:</b> <a href=\"https://solscan.io/account/{}\">{}</a>\n\n\
            <a href=\"https://solscan.io/tx/{}\">View tx</a>",
            s.amm_pool,
            s.amount_in, s.mint_in,
            s.amount_out, s.mint_out,
            route,
            s.signer, self.short(&s.signer),
            s.signature,
        )
    }

    fn fmt_pump_fun(&self, t: &PumpFunTrade) -> String {
        let action = if t.is_buy { "Buy" } else { "Sell" };
        let sol = format!("{:.9}", t.sol_amount as f64 / 1_000_000_000.0);
        format!(
            "🚨 <b>Whale Trade (Pump.fun)</b>\n\n\
            <b>Action:</b> {}\n\
            <b>Mint:</b> <code>{}</code>\n\
            <b>Tokens:</b> {}\n\
            <b>SOL:</b> {} SOL\n\
            <b>User:</b> <a href=\"https://solscan.io/account/{}\">{}</a>\n\n\
            <a href=\"https://solscan.io/tx/{}\">View tx</a>",
            action,
            t.mint,
            t.token_amount,
            sol,
            t.user, self.short(&t.user),
            t.signature,
        )
    }

    fn format_alert(&self, swap: &SwapEvent) -> String {
        match swap {
            SwapEvent::Raydium(s) => self.fmt_raydium(s),
            SwapEvent::Jupiter(s) => self.fmt_jupiter(s),
            SwapEvent::PumpFun(t) => self.fmt_pump_fun(t),
        }
    }
}

#[async_trait]
impl Notifier for TelegramNotifier {
    async fn send_swap_alert(&self, swap: &SwapEvent) -> Result<()> {
        let text = self.format_alert(swap);
        self.bot
            .send_message(self.chat_id.clone(), text)
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .await?;
        Ok(())
    }
}
