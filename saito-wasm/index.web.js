// TODO : convert to .ts file
export default import("./pkg/web")
    .then(saito => {
        console.log("saito : ", saito);
        console.log("saito default : ", saito.default);
        return saito.default(undefined).then(() => saito);
    })
    .catch(error => {
        console.error(error);
    })