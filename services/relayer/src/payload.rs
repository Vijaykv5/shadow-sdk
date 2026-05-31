use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

pub const MAGICBLOCK_LOCAL_ER_VALIDATOR: &str = "mAGicPQYBMvcYveUZA5F5UNNwyHvfYh5xkLS2Fr1mev";
pub const MAGICBLOCK_DEVNET_ASIA_VALIDATOR: &str = "MAS1Dt9qreoRMQ14YQuhg8UTZMMzDdKhmkZMECCzk57";
pub const MAGICBLOCK_DEVNET_EU_VALIDATOR: &str = "MEUGGrYPxKk17hCr7wpT6s8dtNokZj5U2L57vjYMS8e";
pub const MAGICBLOCK_DEVNET_US_VALIDATOR: &str = "MUS3hc9TCw4cGC12vHNoYcCGzJG1txjgQLZWVoeNHNd";
pub const MAGICBLOCK_DEVNET_TEE_VALIDATOR: &str = "MTEWGuqxUpYZGFJQcp8tLN7x5v9BSeoFHYWQQ3n3xzo";

#[derive(Debug, Deserialize, Serialize)]
pub struct IntentPayload {
    pub nonce: u64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub route: Option<serde_json::Value>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadAction {
    MockExecution { message: String },
    SystemTransfer { to: Pubkey, lamports: u64 },
    PerpsOrder(PerpsOrder),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerpsOrder {
    pub venue: PerpsVenue,
    pub market: String,
    pub side: OrderSide,
    pub size_base_lots: u64,
    pub limit_price: u64,
    pub max_slippage_bps: u16,
    pub reduce_only: bool,
    pub client_order_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerpsVenue {
    Mock,
    Drift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Long,
    Short,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionRoute {
    PublicRpc,
    MockPrivateBundle {
        tip_lamports: u64,
    },
    JitoBundle {
        tip_lamports: u64,
    },
    MagicBlockEr {
        validator: MagicBlockValidator,
        commit_frequency_ms: u32,
    },
    MagicBlockPer {
        validator: MagicBlockValidator,
        commit_frequency_ms: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicBlockValidator {
    LocalEr,
    DevnetAsia,
    DevnetEu,
    DevnetUs,
    DevnetTee,
}

pub fn parse_payload_action(intent: &IntentPayload) -> Result<PayloadAction> {
    match intent.kind.as_str() {
        "mock_execution" => {
            #[derive(Deserialize)]
            struct MockPayload {
                message: String,
            }

            let payload = serde_json::from_value::<MockPayload>(intent.payload.clone())
                .context("mock_execution payload must contain string field `message`")?;
            anyhow::ensure!(
                !payload.message.trim().is_empty(),
                "mock_execution message must not be empty"
            );

            Ok(PayloadAction::MockExecution {
                message: payload.message,
            })
        }
        "system_transfer" => {
            #[derive(Deserialize)]
            struct TransferPayload {
                to: Pubkey,
                lamports: u64,
            }

            let payload = serde_json::from_value::<TransferPayload>(intent.payload.clone())
                .context("system_transfer payload must contain `to` and `lamports`")?;
            anyhow::ensure!(
                payload.to != Pubkey::default(),
                "system_transfer recipient must not be the default pubkey"
            );
            anyhow::ensure!(
                payload.lamports > 0,
                "system_transfer lamports must be greater than zero"
            );

            Ok(PayloadAction::SystemTransfer {
                to: payload.to,
                lamports: payload.lamports,
            })
        }
        "perps_order" => {
            #[derive(Deserialize)]
            struct PerpsPayload {
                venue: String,
                market: String,
                side: String,
                size_base_lots: u64,
                limit_price: u64,
                max_slippage_bps: u16,
                reduce_only: Option<bool>,
                client_order_id: String,
            }

            let payload = serde_json::from_value::<PerpsPayload>(intent.payload.clone()).context(
                "perps_order payload must contain venue, market, side, size_base_lots, limit_price, max_slippage_bps, and client_order_id",
            )?;
            let venue = parse_perps_venue(&payload.venue)?;
            let side = parse_order_side(&payload.side)?;
            anyhow::ensure!(
                !payload.market.trim().is_empty(),
                "perps_order market must not be empty"
            );
            anyhow::ensure!(
                payload.size_base_lots > 0,
                "perps_order size_base_lots must be greater than zero"
            );
            anyhow::ensure!(
                payload.limit_price > 0,
                "perps_order limit_price must be greater than zero"
            );
            anyhow::ensure!(
                payload.max_slippage_bps <= 1_000,
                "perps_order max_slippage_bps must be <= 1000"
            );
            anyhow::ensure!(
                !payload.client_order_id.trim().is_empty(),
                "perps_order client_order_id must not be empty"
            );

            Ok(PayloadAction::PerpsOrder(PerpsOrder {
                venue,
                market: payload.market,
                side,
                size_base_lots: payload.size_base_lots,
                limit_price: payload.limit_price,
                max_slippage_bps: payload.max_slippage_bps,
                reduce_only: payload.reduce_only.unwrap_or(false),
                client_order_id: payload.client_order_id,
            }))
        }
        other => anyhow::bail!(
            "unsupported payload kind `{other}`; supported kinds are `mock_execution`, `system_transfer`, and `perps_order`"
        ),
    }
}

pub fn parse_execution_route(intent: &IntentPayload) -> Result<ExecutionRoute> {
    let Some(route) = &intent.route else {
        return Ok(ExecutionRoute::PublicRpc);
    };

    #[derive(Deserialize)]
    struct RoutePayload {
        kind: String,
        tip_lamports: Option<u64>,
        validator: Option<String>,
        commit_frequency_ms: Option<u32>,
    }

    let route = serde_json::from_value::<RoutePayload>(route.clone())
        .context("route must contain string field `kind`")?;

    match route.kind.as_str() {
        "public_rpc" => Ok(ExecutionRoute::PublicRpc),
        "mock_private_bundle" => Ok(ExecutionRoute::MockPrivateBundle {
            tip_lamports: route.tip_lamports.unwrap_or(0),
        }),
        "jito_bundle" => {
            let tip_lamports = route
                .tip_lamports
                .context("jito_bundle route requires `tip_lamports`")?;
            anyhow::ensure!(
                tip_lamports > 0,
                "jito_bundle tip_lamports must be greater than zero"
            );
            Ok(ExecutionRoute::JitoBundle { tip_lamports })
        }
        "magicblock_er" => {
            let validator = parse_magicblock_validator(
                route
                    .validator
                    .as_deref()
                    .unwrap_or("local_er"),
            )?;
            let commit_frequency_ms = route.commit_frequency_ms.unwrap_or(30_000);
            anyhow::ensure!(
                commit_frequency_ms > 0,
                "magicblock_er commit_frequency_ms must be greater than zero"
            );

            Ok(ExecutionRoute::MagicBlockEr {
                validator,
                commit_frequency_ms,
            })
        }
        "magicblock_per" => {
            let validator = parse_magicblock_validator(
                route
                    .validator
                    .as_deref()
                    .context("magicblock_per route requires `validator`")?,
            )?;
            anyhow::ensure!(
                validator.is_tee(),
                "magicblock_per requires a TEE validator such as `devnet_tee`"
            );
            let commit_frequency_ms = route.commit_frequency_ms.unwrap_or(30_000);
            anyhow::ensure!(
                commit_frequency_ms > 0,
                "magicblock_per commit_frequency_ms must be greater than zero"
            );

            Ok(ExecutionRoute::MagicBlockPer {
                validator,
                commit_frequency_ms,
            })
        }
        other => anyhow::bail!(
            "unsupported execution route `{other}`; supported routes are `public_rpc`, `mock_private_bundle`, `jito_bundle`, `magicblock_er`, and `magicblock_per`"
        ),
    }
}

fn parse_magicblock_validator(value: &str) -> Result<MagicBlockValidator> {
    match value {
        "local_er" | MAGICBLOCK_LOCAL_ER_VALIDATOR => Ok(MagicBlockValidator::LocalEr),
        "devnet_asia" | MAGICBLOCK_DEVNET_ASIA_VALIDATOR => Ok(MagicBlockValidator::DevnetAsia),
        "devnet_eu" | MAGICBLOCK_DEVNET_EU_VALIDATOR => Ok(MagicBlockValidator::DevnetEu),
        "devnet_us" | MAGICBLOCK_DEVNET_US_VALIDATOR => Ok(MagicBlockValidator::DevnetUs),
        "devnet_tee" | MAGICBLOCK_DEVNET_TEE_VALIDATOR => Ok(MagicBlockValidator::DevnetTee),
        other => anyhow::bail!(
            "unsupported MagicBlock validator `{other}`; supported validators are `local_er`, `devnet_asia`, `devnet_eu`, `devnet_us`, and `devnet_tee`"
        ),
    }
}

fn parse_perps_venue(venue: &str) -> Result<PerpsVenue> {
    match venue {
        "mock" => Ok(PerpsVenue::Mock),
        "drift" => Ok(PerpsVenue::Drift),
        other => anyhow::bail!(
            "unsupported perps venue `{other}`; supported venues are `mock` and `drift`"
        ),
    }
}

fn parse_order_side(side: &str) -> Result<OrderSide> {
    match side {
        "long" => Ok(OrderSide::Long),
        "short" => Ok(OrderSide::Short),
        other => anyhow::bail!(
            "unsupported perps side `{other}`; supported sides are `long` and `short`"
        ),
    }
}

impl ExecutionRoute {
    pub fn label(&self) -> &'static str {
        match self {
            Self::PublicRpc => "public_rpc",
            Self::MockPrivateBundle { .. } => "mock_private_bundle",
            Self::JitoBundle { .. } => "jito_bundle",
            Self::MagicBlockEr { .. } => "magicblock_er",
            Self::MagicBlockPer { .. } => "magicblock_per",
        }
    }
}

impl MagicBlockValidator {
    pub fn label(self) -> &'static str {
        match self {
            Self::LocalEr => "local_er",
            Self::DevnetAsia => "devnet_asia",
            Self::DevnetEu => "devnet_eu",
            Self::DevnetUs => "devnet_us",
            Self::DevnetTee => "devnet_tee",
        }
    }

    pub fn pubkey(self) -> &'static str {
        match self {
            Self::LocalEr => MAGICBLOCK_LOCAL_ER_VALIDATOR,
            Self::DevnetAsia => MAGICBLOCK_DEVNET_ASIA_VALIDATOR,
            Self::DevnetEu => MAGICBLOCK_DEVNET_EU_VALIDATOR,
            Self::DevnetUs => MAGICBLOCK_DEVNET_US_VALIDATOR,
            Self::DevnetTee => MAGICBLOCK_DEVNET_TEE_VALIDATOR,
        }
    }

    pub fn is_tee(self) -> bool {
        matches!(self, Self::DevnetTee)
    }
}

impl OrderSide {
    pub fn label(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }
}
