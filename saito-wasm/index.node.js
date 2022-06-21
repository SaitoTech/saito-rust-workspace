let saito = require("./pkg/node/index");
// import * as saito from './pkg/node';
// let saito = await import("./pkg/node");
const fetch = require("node-fetch");

global.fetch = fetch;

console.log("saito : ", saito);
// console.log("saito default : ", saito.default);
let result = saito.initialize_sync();
console.log("result = ", result);

export default saito;


// export default import("./pkg/node")
//     .then(saito => {
//         console.log("saito : ", saito);
//         console.log("saito default : ", saito.default);
//         return saito.default(undefined).then(() => saito);
//         // return saito;
//     })
//     .then(saito => {
//         console.log("s = ", saito);
//         let result = saito.initialize_sync();
//         console.log("result = ", result);
//         return saito;
//     })
//     .catch(error => {
//         console.error(error);
//     })