import type {WasmBalanceSnapshot} from "saito-wasm/pkg/node/index";
import WasmWrapper from "./wasm_wrapper";

export default class BalanceSnapshot extends WasmWrapper<WasmBalanceSnapshot> {
    public static Type: any;

    constructor(snapshot?: WasmBalanceSnapshot) {
        if (!snapshot) {
            snapshot = new BalanceSnapshot.Type();
        }
        super(snapshot!);
    }

    public get file_name(): string {
        return this.instance.get_file_name();
    }

    public get rows(): Array<string> {
        return this.instance.get_entries();
    }
}
