use anyhow::{anyhow, Result};
use bitcoin::consensus::encode::deserialize;
use bitcoin::network::constants::ServiceFlags;
use bitcoincore_rpc::{Auth, Client, Error as RpcError, RpcApi};
use cln_plugin::Plugin;
use cln_plugin::{options, Builder};
use hex::FromHex;
use home::home_dir;
use jsonrpc::error::Error as JsonRpcError;
use log::debug;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio;

use broadcast_over_tor::broadcast;

#[derive(Clone)]
pub struct State {
    pub rpc_client: Option<Arc<Client>>,
}

impl State {
    pub fn new() -> Self {
        State { rpc_client: None }
    }
}

async fn send_raw_transaction(
    plugin: Plugin<Arc<Mutex<State>>>,
    v: serde_json::Value,
) -> Result<serde_json::Value> {
    let mut addresses = {
        let state = plugin.state().lock().unwrap();
        let client = state.rpc_client.as_ref().unwrap();

        client
            .get_node_addresses(Some(250))?
            .into_iter()
            .filter(|address| {
                let services = ServiceFlags::from(address.services as u64);
                services.has(ServiceFlags::WITNESS)
            })
            .map(|address| format!("{}:{}", address.address, address.port))
            .collect()
    };

    if let Some(tx) = v["tx"].as_str() {
        let tx_bytes = Vec::from_hex(tx)?;
        let tx = deserialize(&tx_bytes)?;
        let result = broadcast(&tx, &mut addresses).await;
        match result {
            Ok(_) => Ok(json!({"success": true, "errmsg": ""})),
            Err(e) => {
                let state = plugin.state().lock().unwrap();
                let client = state.rpc_client.as_ref().unwrap();
                let result = client.get_raw_transaction(&tx.txid(), None);
                match result {
                    Ok(_) => Ok(json!({"success": true, "errmsg": ""})),
                    Err(_) => Ok(json!({"success": false, "errmsg": e.to_string()})),
                }
            }
        }
    } else {
        let errmsg = format!("Invalid tx sent to sendrawtransaction {:}", v);
        Ok(json!({"success": false, "errmsg": errmsg }))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    debug!("Starting broadcast-over-tor plugin");

    let state = Arc::new(Mutex::new(State::new()));
    let state_clone = state.clone();
    let plugin = match Builder::new(state, tokio::io::stdin(), tokio::io::stdout())
        .option(options::ConfigOption::new(
            "broadcast-over-tor-bitcoin-datadir",
            options::Value::String(home_dir().unwrap().to_str().unwrap().to_string()),
            "bitcoind data dir",
        ))
        .option(options::ConfigOption::new(
            "broadcast-over-tor-bitcoin-rpcport",
            options::Value::Integer(8332),
            "bitcoind rpc host's port",
        ))
        .option(options::ConfigOption::new(
            "broadcast-over-tor-bitcoin-rpcconnect",
            options::Value::String(String::from("127.0.0.1")),
            "bitcoind rpc server url",
        ))
        .option(options::ConfigOption::new(
            "broadcast-over-tor-bitcoin-rpcuser",
            options::Value::String(String::from("user")),
            "bitcoind rpc server user",
        ))
        .option(options::ConfigOption::new(
            "broadcast-over-tor-bitcoin-rpcpassword",
            options::Value::String(String::from("password")),
            "bitcoind rpc server password",
        ))
        .rpcmethod(
            "sendrawtransaction",
            "Send a raw transaction to the Bitcoin network.",
            send_raw_transaction,
        )
        .configure()
        .await?
    {
        Some(p) => p,
        None => return Ok(()),
    };

    let data_dir = match plugin.option("broadcast-over-tor-bitcoin-datadir") {
        Some(options::Value::String(s)) => s,
        None => home_dir().unwrap().to_str().unwrap().to_string(),
        Some(o) => return Err(anyhow!("bitcoin-datadir is not a valid string: {:?}", o)),
    };
    let rpc_port = match plugin.option("broadcast-over-tor-bitcoin-rpcport") {
        Some(options::Value::Integer(s)) => s,
        None => 8332,
        Some(o) => return Err(anyhow!("bitcoin-rpcport is not a valid integer: {:?}", o)),
    };
    let rpc_host = match plugin.option("broadcast-over-tor-bitcoin-rpcconnect") {
        Some(options::Value::String(s)) => s,
        None => String::from("127.0.0.1"),
        Some(o) => return Err(anyhow!("bitcoin-rpcconnect is not a valid string: {:?}", o)),
    };
    let rpc_user = match plugin.option("broadcast-over-tor-bitcoin-rpcuser") {
        Some(options::Value::String(s)) => s,
        None => String::from("user"),
        Some(o) => return Err(anyhow!("bitcoin-rpcuser is not a valid string: {:?}", o)),
    };
    let rpc_password = match plugin.option("broadcast-over-tor-bitcoin-rpcpassword") {
        Some(options::Value::String(s)) => s,
        None => String::from("password"),
        Some(o) => {
            return Err(anyhow!(
                "bitcoin-rpcpassword is not a valid string: {:?}",
                o
            ))
        }
    };

    let client = connect_rpc(data_dir, rpc_host, rpc_port, rpc_user, rpc_password).await?;
    state_clone.lock().unwrap().rpc_client = Some(Arc::new(client));

    let plugin = plugin.start().await?;

    plugin.join().await
}

async fn connect_rpc(
    data_dir: String,
    rpc_host: String,
    rpc_port: i64,
    rpc_user: String,
    rpc_password: String,
) -> Result<Client> {
    loop {
        let path = PathBuf::from(&data_dir).join(".cookie");
        let auth = Auth::CookieFile(path);
        let rpc_url = format!("{}:{}", rpc_host, rpc_port);
        let result = Client::new(&rpc_url, auth);
        let client = match result {
            Ok(client) => client,
            Err(_) => {
                let auth = Auth::UserPass(rpc_user.clone(), rpc_password.clone());
                Client::new(&rpc_url, auth)?
            }
        };

        // TODO: Check response for bitcoind compatability
        match client.get_network_info() {
            Ok(_) => return Ok(client),
            Err(RpcError::JsonRpc(JsonRpcError::Rpc(jsonrpc::error::RpcError {
                code: 28,
                ..
            }))) => {
                debug!("Waiting for bitcoind to warm up...");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
            Err(_) => {
                return Err(anyhow!(
                    "Could not connect to bitcoind. Is bitcoind running?"
                ));
            }
        }
    }
}
