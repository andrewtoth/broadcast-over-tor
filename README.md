# Broadcast Over Tor

A CoreLightning bitcoin plugin that sends all on-chain transactions to random
nodes via tor. This is useful for preserving privacy if running your lightning
node over tor but your bitcoin node on clearnet. 

While the lightning network cannot learn of your node's IP, spies can still 
learn the originating IP of your bitcoin node when sending on-chain 
transactions. If all transactions from your lightning node originate from the 
same clearnet bitcoin node, it can be inferred that your lightning node is at 
the same location.

Instead of sending via bitcoind's `sendrawtransaction` RPC, this plugin makes
multiple independent outbound p2p connections to random nodes and sends the 
transaction to them via the p2p protocol. It gets these addresses using
bitcoind's `getnodeaddresses` RPC.

Now you can still enjoy all the speed and reliability benefits of using bitcoind
over clearnet, without concern that spies will learn the IP of your lightning
node.

## Installation

Since this plugin only implements the `sendrawtransaction` backend method, you
must use it in conjunction with one or more backend plugins that implements
the other four methods. Using 
[`rust-bcli`](https://github.com/andrewtoth/rust-bcli/) is one such option.

The following commands build the plugin binary:
```
git clone https://github.com/andrewtoth/broadcast-over-tor
cd broadcast-over-tor
cargo install --path .
```

The binary will now be at `$HOME/.cargo/bin/broadcast-over-tor`. You can now
place this binary into the plugins folder, add it to the conf file with
`plugin=$HOME/.cargo/bin/broadcast-over-tor` (replace `$HOME` with your home 
directory), or add it as a command line option via 
`lightningd --plugin=$HOME/.cargo/bin/broadcast-over-tor`. You must specify 
either the bitcoind data directory for the cookie file or the username and
password for RPC authentication. The is accomplished via the plugin options 
`broadcast-over-tor-bitcoin-datadir`, `broadcast-over-tor-bitcoin-rpcuser` and
`broadcast-over-tor-bitcoin-rpcpassword`.
