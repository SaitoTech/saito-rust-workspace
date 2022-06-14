const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');
const webpack = require('webpack');
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");
const {merge} = require("webpack-merge");

let common = {
    entry: "./index.ts",
    devtool: "inline-source-map",
    plugins: [
        new HtmlWebpackPlugin(),
        new WasmPackPlugin({
            crateDirectory: __dirname,
            extraArgs: '--target bundler',
        }),
        new webpack.ProvidePlugin({
            TextDecoder: ['text-encoding', 'TextDecoder'],
            TextEncoder: ['text-encoding', 'TextEncoder']
        })
    ],
    module: {
        rules: [{
            test: /\.tsx?$/,
            loader: "ts-loader",
            exclude: /(node_modules)/
        }]
    },
    resolve: {
        extensions: ['.ts', '.js', '.wasm']
    },
    experiments: {
        asyncWebAssembly: true,
        topLevelAwait: true,
        // syncWebAssembly: true
    },
    mode: "development"
};

let nodeConfigs = merge(common, {
    output: {
        path: path.resolve(__dirname, "dist"),
        filename: "index.node.js",
    },
    target: "node"
});
let webConfigs = merge(common, {
    output: {
        path: path.resolve(__dirname, "dist"),
        filename: "index.web.js",
    },
    target: "web"
});

module.exports = [nodeConfigs];
