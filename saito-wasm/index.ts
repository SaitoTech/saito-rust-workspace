const saito = import("./pkg");

export default saito
    .then(saito => {
        // console.log("saito loaded : ", saito);
        let result = saito.initialize_sync();
        console.log("result = ", result);
        return saito;
    }).catch(error => {
        console.error(error);
        throw error;
    });
