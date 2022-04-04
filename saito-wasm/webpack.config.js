const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');
const webpack = require('webpack');
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");

module.exports = {
    entry: "./index.js",
    output: {
        path: path.resolve(__dirname, "dist"),
        filename: "index.js",
    },
    plugins: [
        // new HtmlWebpackPlugin(),
        new WasmPackPlugin({
            crateDirectory: __dirname
            // extraArgs: '--no-typescript',
        }),
    ],
    module: {
        // rules: [{
        //     test: /\.tsx?$/,
        //     loader: "ts-loader",
        //     exclude: /(node_modules)/
        // }]
    },
    experiments: {
        asyncWebAssembly: true,
        syncWebAssembly: true
    },
    mode: "production"
};

