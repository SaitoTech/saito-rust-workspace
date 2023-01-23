export default import("./pkg/web")
    .then(saito => {
        console.log("saito : ", saito);
        // @ts-ignore
        console.log("saito default : ", saito.default);
        // @ts-ignore
        return saito.default(undefined).then(() => saito);
        // return saito;
    })
    .catch(error => {
        console.error(error);
    })
