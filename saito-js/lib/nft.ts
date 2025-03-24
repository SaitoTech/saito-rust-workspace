import type { WasmNFT } from "saito-wasm/pkg/node/index";
import WasmWrapper from "./wasm_wrapper";

export default class Nft extends WasmWrapper<WasmNFT> {
    public static Type: any;

    constructor(nft?: WasmNFT) {
        if (!nft) {
            nft = new Nft.Type();
        }
        super(nft!);
    }

    public get nft_id(): Uint8Array {
        return this.instance.nft_id;
    }

    public get utxokey_bound(): Uint8Array {
        return this.instance.utxokey_bound;
    }

    public get utxokey_normal(): Uint8Array {
        return this.instance.utxokey_normal;
    }

    public get tx_sig(): Uint8Array {
        return this.instance.tx_sig;
    }

    public static fromString(str: string): Nft | null {
        try {
            let nft = this.Type.from_string?.(str);
            return nft ? new Nft(nft) : null;
        } catch (error) {
            console.error(error);
            return null;
        }
    }

    // Convert NFT data to a JSON-friendly format
    public toJSON(): Record<string, string> {
        return {
            nft_id: Buffer.from(this.nft_id).toString("hex"),
            utxokey_bound: Buffer.from(this.utxokey_bound).toString("hex"),
            utxokey_normal: Buffer.from(this.utxokey_normal).toString("hex"),
            tx_sig: Buffer.from(this.tx_sig).toString("hex"),
        };
    }
}
