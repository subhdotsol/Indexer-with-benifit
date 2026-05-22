use anyhow::{ Result};
use teloxide::{Bot, payloads::SendMessageSetters, prelude::Requester, sugar::request::RequestLinkPreviewExt, types::{ChatId, ParseMode, Recipient}};

use crate::{application::Notifier, domain::{JupiterSwapEvent, RaydiumSwapEvent, SwapEvent}};
use async_trait::async_trait;

pub struct TelegramAdaptor{
    bot:Bot,
    chat_id:Recipient,
}

impl TelegramAdaptor{
    pub fn new(bot_token:String,chat_id_str:String)->Self{
        let bot = Bot::new(bot_token);
        // parsing the chat id
        let chat_id = match chat_id_str.parse::<i64>(){
            Ok(id)=>{
                Recipient::Id(ChatId(id))
            },
            Err(_)=>{
                Recipient::ChannelUsername(chat_id_str)
            }
        };
        Self { bot, chat_id }
    }


    fn short_key(&self, key: &str) -> String {
        key.chars().take(8).collect()
    }

    fn format_raydium_message(&self, swap: &RaydiumSwapEvent) -> String {
        format!(
            "🚨 <b>Whale Swap Detected (Raydium)!</b>\n\n\
            <b>Pool:</b> <code>{}</code>\n\
            <b>In:</b> {} <code>{}</code>\n\
            <b>Out:</b> {} <code>{}</code>\n\
            <b>Signer:</b> <a href=\"https://solscan.io/account/{}\">{}</a>\n\n\
            <a href=\"https://solscan.io/tx/{}\">View Transaction</a>",
            swap.amm_pool,
            swap.amount_in, swap.mint_source,
            swap.amount_received, swap.mint_destination,
            swap.signer,
            self.short_key(&swap.signer),
            swap.signature
        )
    }

    fn format_jupiter_message(&self, swap: &JupiterSwapEvent) -> String {
        let route = if swap.route_plan.is_empty() {
            "-".to_string()
        } else {
            swap.route_plan
                .iter()
                .map(|step| format!("{} ({}%)", step.swap_label, step.percent))
                .collect::<Vec<_>>()
                .join(" -> ")
        };

        format!(
            "🚨 <b>Whale Swap Detected (Jupiter)!</b>\n\n\
            <b>Pool:</b> <code>{}</code>\n\
            <b>In:</b> {} <code>{}</code>\n\
            <b>Out:</b> {} <code>{}</code>\n\
            <b>Route:</b> {}\n\
            <b>Signer:</b> <a href=\"https://solscan.io/account/{}\">{}</a>\n\n\
            <a href=\"https://solscan.io/tx/{}\">View Transaction</a>",
            swap.amm_pool,
            swap.amount_in, swap.mint_in,
            swap.amount_out, swap.mint_out,
            route,
            swap.signer,
            self.short_key(&swap.signer),
            swap.signature
        )
    }

    fn format_pumpfun_message(&self, trade: &crate::domain::PumpFunTrade) -> String {
        let action = if trade.is_buy { "Buy" } else { "Sell" };
        let sol_lamports_str = format!("{:.9}", trade.sol_amount as f64 / 1_000_000_000.0);
        
        format!(
            "🚨 <b>Whale Trade Detected (Pump.fun)!</b>\n\n\
            <b>Action:</b> {}\n\
            <b>Mint:</b> <code>{}</code>\n\
            <b>Token Amount:</b> {}\n\
            <b>SOL Amount:</b> {} SOL\n\
            <b>User:</b> <a href=\"https://solscan.io/account/{}\">{}</a>\n\n\
            <a href=\"https://solscan.io/tx/{}\">View Transaction</a>",
            action,
            trade.mint,
            trade.token_amount,
            sol_lamports_str,
            trade.user,
            self.short_key(&trade.user),
            trade.signature
        )
    }

    fn format_message(&self, swap: &SwapEvent) -> String {
        match swap {
            SwapEvent::Raydium(event) => self.format_raydium_message(event),
            SwapEvent::Jupiter(event) => self.format_jupiter_message(event),
            SwapEvent::PumpFun(trade) => self.format_pumpfun_message(trade),
        }
    }
}

#[async_trait]
impl Notifier for TelegramAdaptor{
    async fn send_swap_alert(&self,swap:&SwapEvent)->Result<()>{
        let text = self.format_message(swap);

        self.bot.send_message(self.chat_id.clone(), text)
        .parse_mode(ParseMode::Html)
        .disable_link_preview(true)
        .await?;

        Ok(())
    }
} 