// const s = await import("./pkg");
// import * as s from 'saito-wasm';
// import * as s from './pkg/index';
//
// console.log("lib loaded before init");
//
// let result = s.initialize_sync();
// console.log("result = ", result);

// export * from './pkg';

// @ts-ignore
export default import("./pkg/index")
    .then(saito => {
        // console.log("saito loaded : ", saito);
        let result = saito.initialize_sync();
        console.log("result = ", result);
        return saito;
    })
    .catch(error => {
        console.error(error);
        throw error;
    });
