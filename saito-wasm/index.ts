// const s = await import("./pkg");
import * as s from 'saito-wasm';
//
// console.log("lib loaded before init");
//
let result = s.initialize_sync();
console.log("result = ", result);

// export * from './pkg';

export default s
// .then(saito => {
//     // console.log("saito loaded : ", saito);
//     let result = saito.initialize_sync();
//     console.log("result = ", result);
//     return saito;
// })
// .catch(error => {
//     console.error(error);
//     throw error;
// });
