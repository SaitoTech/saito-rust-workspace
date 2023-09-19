
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


