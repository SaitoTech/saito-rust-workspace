
# Welcome to Saito

This distribution contains an implementation of Saito Consensus in the Rust programming language. It includes the core code needed to run Saito in any language. It contains three main components organized by three main sub-directories: saito-core, saito-rust, and saito-wasm.

*** Saito-Core ***

This is the place to start if you are interested in understanding Saito Consensus. The code in this directory is used by all versions of Saito. It constitutes the basic classes (blocks, transactions, mempool) that process the blockchain as well as the universal local for processing the events that run the blockchain and keep the network in sync.


*** Saito-Rust ***

The Saito Rust directory contains the code needed to provide storage and network services in the Rust language. These handlers are passed into Saito-Core, where the core software will occasionally call them in order to process network or storage requests such as saving blocks to disk or sending messages to other peers on the network.

You should only need to modify these files if you are developing features that affect storage or network functionality in the Rust-only client.


*** Saito-Wasm ***

Saito provides a way to compile our shared library into WASM, a type of binary code that can be executed in many other languages and other platforms. An example of this is compiling Saito into WASM for deployment in a browser -- allowing the network and browser code to run lite-clients that use the same underlying code so as to prevent accidental forks.

