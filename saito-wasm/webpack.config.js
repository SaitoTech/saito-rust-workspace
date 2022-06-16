const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');
const webpack = require('webpack');
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");
const {merge} = require("webpack-merge");

let common = {
    entry: "./index.ts",
    devtool: "eval",
    output: {
        path: path.resolve(__dirname, "dist"),
        filename: "index.js",
    },
    plugins: [
        new HtmlWebpackPlugin(),
        new WasmPackPlugin({
            crateDirectory: __dirname,
            extraArgs: '--target web',
        }),
        new webpack.ProvidePlugin({
            TextDecoder: ['text-encoding', 'TextDecoder'],
            TextEncoder: ['text-encoding', 'TextEncoder']
        })
    ],
    module: {
        rules: [
            {
                test: /\.js$/,
                use: [
                    "source-map-loader",
                    {
                        loader: "babel-loader",
                        options: {
                            presets: ["@babel/preset-env"],
                            sourceMaps: true
                        }
                    }
                ],
                exclude: /(node_modules)/
            },
            {
                test: /\.tsx?$/,
                loader: "ts-loader",
                exclude: /(node_modules)/
            },
            {
                test: /\.wasm$/,
                type: "javascript/auto",
                loader: "file-loader",
                options: {
                    publicPath: "dist/"
                }
            },
        ]
    },
    // resolve: {
    //     extensions: ['.ts', '.tsx', '.js', '.wasm', '...']
    // },
    experiments: {
        asyncWebAssembly: true,
        // topLevelAwait: true,
        syncWebAssembly: true
    },
    mode: "development"
};

let nodeConfigs = merge(common, {
    output: {
        path: path.resolve(__dirname, "dist"),
        filename: "server.js",
    },
    target: "node"
});
let webConfigs = merge(common, {
    output: {
        path: path.resolve(__dirname, "dist"),
        filename: "browser.js",
    },
    target: "web",
});

module.exports = [nodeConfigs, webConfigs];
