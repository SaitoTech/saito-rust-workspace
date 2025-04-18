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

    public get nft_slip2_utxokey(): Uint8Array {
        return this.instance.nft_slip2_utxokey;
    }

    public get nft_slip1_utxokey(): Uint8Array {
        return this.instance.nft_slip1_utxokey;
    }

    public get normal_slip_utxokey(): Uint8Array {
        return this.instance.normal_slip_utxokey;
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
            nft_slip2_utxokey: Buffer.from(this.nft_slip2_utxokey).toString("hex"),
            nft_slip1_utxokey: Buffer.from(this.nft_slip1_utxokey).toString("hex"),
            normal_slip_utxokey: Buffer.from(this.normal_slip_utxokey).toString("hex"),
            tx_sig: Buffer.from(this.tx_sig).toString("hex"),
        };
    }
}
