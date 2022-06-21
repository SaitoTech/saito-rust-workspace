const path = require('path');
const HtmlWebpackPlugin = require('html-webpack-plugin');
const webpack = require('webpack');
const WasmPackPlugin = require("@wasm-tool/wasm-pack-plugin");
const {merge} = require("webpack-merge");

let common = {
    devtool: "eval",
    // entry: [
    //     path.resolve(__dirname, "./index.js"),
    // ],
    output: {
        path: path.resolve(__dirname, "dist"),
        filename: "index.js",
    },
    // plugins: [
    //     new HtmlWebpackPlugin(),
    //     new WasmPackPlugin({
    //         crateDirectory: __dirname,
    //         extraArgs: '--target web',
    //     }),
    //     new webpack.ProvidePlugin({
    //         TextDecoder: ['text-encoding', 'TextDecoder'],
    //         TextEncoder: ['text-encoding', 'TextEncoder']
    //     })
    // ],
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
                test: /\.mjs$/,
                include: /node_modules/,
                type: "javascript/auto"
            },
            {
                test: /\.tsx?$/,
                loader: "ts-loader",
                exclude: /(node_modules)/
            },
            // {
            //     test: /\.wasm$/,
            //     type: "javascript/auto",
            //     loader: "file-loader",
            //     options: {
            //         publicPath: "dist/"
            //     }
            // },
            {
                test: /\.wasm$/,
                type: "asset/inline",
            },
        ],
        parser: {
            javascript: {
                dynamicImportMode: 'eager'
            }
        }
    },
    resolve: {
        extensions: ['.ts', '.tsx', '.js', '.wasm', '...'],
        fallback: {
            "buffer": require.resolve("buffer")
        }
    },
    experiments: {
        asyncWebAssembly: true,
        // topLevelAwait: true,
        syncWebAssembly: true,
        // lazyCompilation: false,
        // outputModule: false,
    },
    mode: "production",
};

let nodeConfigs = merge(common, {
    entry: [
        'babel-regenerator-runtime',
        path.resolve(__dirname, "./index.node.js"),
    ],
    plugins: [
        new HtmlWebpackPlugin(),
        new WasmPackPlugin({
            crateDirectory: __dirname,
            outDir: "./pkg/node",
            extraArgs: '--target nodejs',
        }),
        new webpack.ProvidePlugin({
            TextDecoder: ['text-encoding', 'TextDecoder'],
            TextEncoder: ['text-encoding', 'TextEncoder']
        })
    ],
    output: {
        path: path.resolve(__dirname, "dist/server"),
        filename: "index.js",
    },
    target: "node"
});
let webConfigs = merge(common, {
    entry: [
        'babel-regenerator-runtime',
        path.resolve(__dirname, "./index.web.js"),
    ],
    plugins: [
        new HtmlWebpackPlugin(),
        new WasmPackPlugin({
            crateDirectory: __dirname,
            outDir: "./pkg/web",
            extraArgs: '--target web',
        }),
        new webpack.ProvidePlugin({
            TextDecoder: ['text-encoding', 'TextDecoder'],
            TextEncoder: ['text-encoding', 'TextEncoder']
        })
    ],
    output: {
        path: path.resolve(__dirname, "dist/browser"),
        filename: "index.js",
    },
    target: "web",
});

module.exports = [nodeConfigs, webConfigs];
