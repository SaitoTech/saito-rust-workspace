// let saito = require("./pkg/node/index");
// import * as saito from './pkg/node';
// let saito = await import("./pkg/node");
// @ts-ignore
// const fetch = require("node-fetch");
//
// global.fetch = fetch;
let saito;
export default import("./pkg/node").then(s => {
    saito = s;
    // console.log("saito : ", saito);
    return saito;
});
// console.log("saito : ", saito);
// console.log("saito default : ", saito.default);
// let result = saito.initialize_sync();
// console.log("result = ", result);


// export default () => saito;
// export default async () => {
//     const saito = await import("./pkg/node");
//     return saito;
// }
//
// function initialize() {
//     return new Promise((resolve, reject) => {
//         return import("./pkg/node")
//             .then(saito => {
//                 resolve(resolve);
//             }).catch(error => reject(error));
//     })
//     // .then(saito => {
//     //     console.log("saito : ", saito);
//     //     // console.log("saito default : ", saito.default);
//     //     // return saito.default(undefined).then(() => saito);
//     //     return saito;
//     // })
//     // .then(saito => {
//     //     console.log("s = ", saito);
//     //     let result = saito.initialize_sync();
//     //     console.log("result = ", result);
//     //     return saito;
//     // })
//     // .catch(error => {
//     //     console.error(error);
//     // });
// }
//
// export default {
//     init: "init11"
// };