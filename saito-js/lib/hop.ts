// Import the necessary WebAssembly functionality
import type { WasmHop } from "saito-wasm/pkg/node/index";
import WasmWrapper from "./wasm_wrapper";

export default class Hop extends WasmWrapper<WasmHop> {
    public static Type: any;

    constructor(hop?: WasmHop, json?: any) {
        if (!hop) {
            hop = new Hop.Type();
        }
        super(hop!);

        if (json) {
            this.from = json.from;
            this.to = json.to
            this.sig = json.sig
        }
    }

    public set from(value: string) {
        this.instance.from = value;
    }

    public get from(): string {
        return this.instance.from;
    }


    public set to(value: string) {
        this.instance.to = value;
    }
    public get to(): string {
        return this.instance.to;
    }

    public set sig(value: string) {
        this.instance.sig = value;
    }
    public get sig(): string {
        return this.instance.sig;
    }
    public toJson(): {
        from: string;
        to: string;
        sig: string;
    } {
        return {
            from: this.from,
            to: this.to,
            sig: this.sig,

        };
    }
}
