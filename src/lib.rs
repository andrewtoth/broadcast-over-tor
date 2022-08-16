use anyhow::{anyhow, Result};
use bitcoin::Transaction;
use bitcoin_send_tx_p2p::send_tx_p2p_over_tor;
use log::{info, trace};
use tokio;
use tokio::sync::mpsc::{self, Sender};

const MAX_WORKERS: usize = 32;
const MIN_SUCCESSFUL: u8 = 4;

pub async fn broadcast(transaction: &Transaction, addresses: &mut Vec<String>) -> Result<()> {
    let (tx, mut rx) = mpsc::channel(MAX_WORKERS);
    for i in 1..MAX_WORKERS {
        trace!("Spawning worker {}", i);
        worker(transaction, addresses, &tx)?;
        if addresses.len() == 0 {
            break;
        }
    }

    let mut success_count = 0;
    while let Some(result) = rx.recv().await {
        if let Ok(_) = result {
            success_count += 1;
            trace!("Sent tx successfully to {} nodes", success_count);
            if success_count >= MIN_SUCCESSFUL {
                break;
            }
        } else if addresses.len() > 0 {
            trace!("Spawning another worker after failed send");
            worker(transaction, addresses, &tx)?;
        }
    }
    info!("Broadcast succesfully");

    Ok(())
}

fn worker(
    transaction: &Transaction,
    addresses: &mut Vec<String>,
    sender: &Sender<Result<()>>,
) -> Result<()> {
    let tx_clone = sender.clone();
    let transaction_clone = transaction.clone();
    if let Some(address) = addresses.pop() {
        trace!("Sending to {}", address);
        tokio::spawn(async move {
            let address_clone = address.clone();
            let result = send_tx_p2p_over_tor(address, transaction_clone, None).await;
            match result {
                Ok(_) => trace!("Sent tx successfully to {}", address_clone),
                Err(_) => trace!("Failed sending to {}", address_clone),
            }
            let _ = tx_clone.send(result).await;
        });
    } else {
        return Err(anyhow!("Ran out of addresses"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {

    use super::broadcast;

    use anyhow::Result;
    use bitcoin::consensus::encode::deserialize;
    use bitcoin::network::constants::ServiceFlags;
    use bitcoincore_rpc::{Auth, Client, RpcApi};
    use hex::FromHex;

    use env_logger;

    #[tokio::test]
    async fn test_tor() -> Result<()> {
        let _ = env_logger::builder().is_test(true).try_init();

        let tx_bytes = Vec::from_hex("000000800100000000000000000000000000000000000000000000000000000000000000000000000000ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000")?;
        let tx = deserialize(&tx_bytes)?;

        let client = Client::new(
            "192.168.2.11:8332",
            Auth::UserPass(String::from("user"), String::from("password")),
        )?;

        let mut addresses = client
            .get_node_addresses(Some(100))?
            .into_iter()
            .filter(|address| {
                ServiceFlags::from(address.services as u64).has(ServiceFlags::WITNESS)
            })
            .map(|address| format!("{}:{}", address.address, address.port))
            .collect();

        let result = broadcast(&tx, &mut addresses).await;

        tokio_test::assert_ok!(result, "Broadcast tor failed");
        Ok(())
    }
}
