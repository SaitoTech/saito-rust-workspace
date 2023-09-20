
 # Overview
This guide provides a detailed walkthrough on how to link the saito-lite-rust repository to the saito-wasm locally via the saito-js wrapper. This process leverages the npm link command.

Note: The saito-lite-rust (SLR) repository by default comes bundled with the saito-js library in its package.json. If, however, you wish to manually establish this linkage, the instructions below will guide you.

## Prerequisites

Before starting, ensure:

- Node.js and npm are installed.
- [saito-lite-rust](https://github.com/SaitoTech/saito-lite-rust), [saito-js](https://github.com/SaitoTech/saito-rust-workspace), and [saito-wasm](https://github.com/SaitoTech/saito-rust-workspace) repositories are cloned on your local machine.

## Installation Guide

## Step 1: Prepare `saito-wasm` for Linking

#### Navigate to the saito-wasm directory

#### Create a symbolic link for saito-wasm in the global node_modules after installation and building
```
npm install
npm run build
npm link 
```

## Step 2: Link saito-js to saito-wasm
#### Navigate to the saito-js directory

#### Link to saito-wasm build and create a symbolic link for saito-js
```
npm install
npm link saito-wasm
npm run build
npm link
```

## Step 3: Link SLR to saito-js

#### Navigate to the SLR directory

#### Link to saito-js
``` 
npm install
npm link saito-js
npm run go
```


