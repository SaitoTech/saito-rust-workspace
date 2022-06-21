// const s = await import(/* webpackMode: "eager" */ "./pkg");
// // import * as s from 'saito-wasm';
// // @ts-ignore
// // import * as s from 'saito-wasm-temp/saito_wasm';
// //
// // console.log("lib loaded before init");
// //
// console.log(s);
// await new Promise((done, fail) => {
//     let interval = setInterval(() => {
//         if (typeof s.initialize_sync !== "undefined") {
//             console.log("lib loaded");
//             clearInterval(interval);
//             done();
//         }
//         console.log("waiting for lib to load");
//     }, 100);
//
// });
// let result = s.initialize_sync();
// console.log("result = ", result);
//
// // export * from './pkg';
//
// // @ts-ignore


// let saito = await import("./pkg");
// console.log("saito loaded : ", saito);
// console.log("fn = ", saito.initialize_sync);
// try {
//     let result = saito.initialize_sync();
//     console.log("result = ", result);
// } catch (error) {
//     console.error(error);
// }
//
// export default saito;
// })
// .catch(error => {
//     console.warn("failed loading lib");
//     console.error(error);
//     // throw error;
// });
//
// export default s;
// console.log("loading saito wasm package");
// export * from './pkg/index';
export default import("./pkg/index")
    .then(saito => {
        console.log("saito : ", saito);
        // console.log("saito default : ", saito.default);
        let result = saito.initialize_sync();
        console.log("result = ", result);
        return saito;
    })
    .catch(error => {
        console.error(error);
    })