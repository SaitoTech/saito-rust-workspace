// const handler = import('./pkg/node/snippets/saito-wasm-10de7e358f6ef4db/js/msg_handler');


export default import("./pkg/node").then(s => {
    // return s.default(undefined);
    return s;
}).catch(error => {
    console.error(error);
});
