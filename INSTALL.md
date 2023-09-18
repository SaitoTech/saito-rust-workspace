# saito-rust-workspace
<todo: introduction>

# Node Setup and Installation

## Required Software:

1. cargo / rust [download](https://www.rust-lang.org/tools/install)
2. wasm-pack [download](https://rustwasm.github.io/wasm-pack/installer/)

You will need standard build-tools to compile and install the above software packages. These 
are bundled with X-Code in Mac and are included with most Linux distributions. On Ubuntu you
can install them as follows:

```
   On ubuntu: sudo apt-get update && sudo apt install build-essential pkg-config libssl-dev
```

## Download Saito

git clone git@github.com:SaitoTech/saito-rust-workspace.git

> Use https for deployment.

## Install Saito

1. Navigate into the directory: `cd saito-rust-workspace/`
2. Run `cp configs/saito.config.template.json configs/saito.config.json` and do the necessary changes in saito.config.json.
3. run `RUST_LOG=debug cargo run`

#### Environment Variables

- RUST_LOG - `error,warn,info,debug,trace` Log level of the node
- GEN_TX - If this is "1" will generate test transactions within the node

## Compiling WASM code

1. Go to saito-wasm directory
2. Execute `wasm-pack build --debug --target browser`



# Saito Project Documentation

# Overview
This guide outlines the steps to link SLR to `saito-wasm` locally through the `saito-js` wrapper using `npm link`.

## Prerequisites

Before starting, ensure:

- Node.js and npm are installed.
- `saito-lite-rust`, `saito-js`, and `saito-wasm` repositories are cloned on your local machine.

## Linking SLR to saito-wasm via saito-js

## Step 1: Prepare `saito-wasm` for Linking

#### Navigate to the saito-wasm directory
``` 
cd path/to/saito-wasm
 ```

#### Create a symbolic link for saito-wasm in the global node_modules
```
npm install
npm run build
npm link 
```

## Step 2: Link saito-js to saito-wasm
#### Navigate to the saito-js directory
```
cd path/to/saito-js
```

#### Link to saito-wasm and create a symbolic link for saito-js
```
npm install
npm link saito-wasm
npm run build
npm link
```

## Step 3: Link SLR to saito-js

#### Navigate to the SLR directory
```
cd path/to/SLR
```


#### Link to saito-js
``` 
npm install
npm link saito-js
npm run go
```


