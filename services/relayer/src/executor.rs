use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig, signature::Signer, system_instruction,
    transaction::Transaction,
};

use crate::payload::{ExecutionRoute, PayloadAction, PerpsOrder, PerpsVenue};

pub fn execute_payload_action(
    rpc_client: &RpcClient,
    action: &PayloadAction,
    route: &ExecutionRoute,
    executor: &solana_sdk::signature::Keypair,
) -> Result<()> {
    validate_route_policy(action, route)?;

    match action {
        PayloadAction::MockExecution { message } => {
            println!("mock execution over {}: {message}", route.label());
            Ok(())
        }
        PayloadAction::SystemTransfer { to, lamports } => {
            anyhow::ensure!(
                matches!(
                    route,
                    ExecutionRoute::PublicRpc | ExecutionRoute::MockPrivateBundle { .. }
                ),
                "system_transfer only supports public_rpc and mock_private_bundle routes until route adapters are wired"
            );
            let from = executor.pubkey();
            let instruction = system_instruction::transfer(&from, to, *lamports);
            let recent_blockhash = rpc_client
                .get_latest_blockhash()
                .context("failed to fetch latest blockhash for system transfer")?;
            let transaction = Transaction::new_signed_with_payer(
                &[instruction],
                Some(&from),
                &[executor],
                recent_blockhash,
            );

            rpc_client
                .send_and_confirm_transaction_with_spinner_and_commitment(
                    &transaction,
                    CommitmentConfig::confirmed(),
                )
                .context("failed to send and confirm system transfer")?;

            Ok(())
        }
        PayloadAction::PerpsOrder(order) => execute_perps_order(order, route),
    }
}

pub fn validate_route_policy(action: &PayloadAction, route: &ExecutionRoute) -> Result<()> {
    if matches!(action, PayloadAction::PerpsOrder(_)) && matches!(route, ExecutionRoute::PublicRpc)
    {
        anyhow::bail!(
            "perps_order requires a private/bundle route; use `mock_private_bundle` for local development or `jito_bundle` once a block-engine adapter is configured"
        );
    }

    Ok(())
}

fn execute_perps_order(order: &PerpsOrder, route: &ExecutionRoute) -> Result<()> {
    match (order.venue, route) {
        (PerpsVenue::Mock, ExecutionRoute::MockPrivateBundle { tip_lamports }) => {
            println!(
                "mock perps order: market={} side={} size_base_lots={} limit_price={} max_slippage_bps={} reduce_only={} client_order_id={} route=mock_private_bundle tip_lamports={}",
                order.market,
                order.side.label(),
                order.size_base_lots,
                order.limit_price,
                order.max_slippage_bps,
                order.reduce_only,
                order.client_order_id,
                tip_lamports
            );
            Ok(())
        }
        (PerpsVenue::Drift, ExecutionRoute::JitoBundle { tip_lamports }) => {
            anyhow::bail!(
                "drift perps execution over jito_bundle is schema-ready but not wired to a Drift/Jito adapter yet; requested tip_lamports={tip_lamports}"
            )
        }
        (
            _,
            ExecutionRoute::MagicBlockEr {
                validator,
                commit_frequency_ms,
            },
        ) => {
            anyhow::bail!(
                "MagicBlock ER route accepted for validator={} ({}) with commit_frequency_ms={}, but the stealth-vault program still needs MagicBlock delegation/commit hooks before relayer execution can route through ER",
                validator.label(),
                validator.pubkey(),
                commit_frequency_ms
            )
        }
        (
            _,
            ExecutionRoute::MagicBlockPer {
                validator,
                commit_frequency_ms,
            },
        ) => {
            anyhow::bail!(
                "MagicBlock PER route accepted for TEE validator={} ({}) with commit_frequency_ms={}, but the stealth-vault program still needs Permission Program and delegation hooks before private ER execution is wired",
                validator.label(),
                validator.pubkey(),
                commit_frequency_ms
            )
        }
        (PerpsVenue::Drift, ExecutionRoute::MockPrivateBundle { .. }) => {
            anyhow::bail!(
                "drift venue cannot execute on mock_private_bundle; use venue `mock` for local tests until Drift adapter is wired"
            )
        }
        (_, ExecutionRoute::JitoBundle { .. }) => {
            anyhow::bail!(
                "jito_bundle route is schema-ready but not wired to a block-engine adapter yet"
            )
        }
        (_, ExecutionRoute::PublicRpc) => unreachable!("route policy rejects public perps orders"),
    }
}
