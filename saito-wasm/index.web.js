// import * as saito from './pkg/web';
// const init = await import("./pkg/web");
// console.log(init);
// let saito = await init();
// console.log("saito = ", saito);
//
// let result = saito.initialize_sync();
//
// console.log("re = ", result);
//
// export default saito;

// export * from './pkg/web';

// import * as saito from './pkg/web';
//
//
export default import("./pkg/web")
    .then(saito => {
        console.log("saito : ", saito);
        console.log("saito default : ", saito.default);
        return saito.default(undefined).then(() => saito);
    })
    // .then(saito => {
    //     console.log("s = ", saito);
    //     let result = saito.initialize_sync();
    //     console.log("result = ", result);
    //     return saito;
    // })
    .catch(error => {
        console.error(error);
    })