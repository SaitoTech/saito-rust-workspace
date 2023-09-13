import type {WasmWallet} from "saito-wasm/pkg/node/index";
import {WasmWalletSlip} from "saito-wasm/pkg/node/index";
import Saito from "../saito";
import WasmWrapper from "./wasm_wrapper";
import {fromBase58, toBase58} from "./util";

export const DefaultEmptyPrivateKey =
    "0000000000000000000000000000000000000000000000000000000000000000";

export const DefaultEmptyPublicKey =
    "000000000000000000000000000000000000000000000000000000000000000000";

export const DefaultEmptyBlockHash =
    "0000000000000000000000000000000000000000000000000000000000000000";

export default class Wallet extends WasmWrapper<WasmWallet> {
    public static Type: any;

    constructor(wallet: WasmWallet) {
        super(wallet);
    }

    public async save() {
        return this.instance.save();
    }

    public async load() {
        return this.instance.load();
    }

    public async reset() {
        return this.instance.reset();
    }

    public async getPublicKey() {
        let key = await this.instance.get_public_key();
        return key === DefaultEmptyPublicKey ? "" : toBase58(key);
    }

    public async setPublicKey(key: string) {
        if (key === "") {
            key = DefaultEmptyPublicKey;
        }
        return this.instance.set_public_key(fromBase58(key));
    }

    public async getPrivateKey() {
        let key = await this.instance.get_private_key();
        return key === DefaultEmptyPrivateKey ? "" : key;
    }

    public async setPrivateKey(key: string) {
        if (key === "") {
            key = DefaultEmptyPrivateKey;
        }
        return this.instance.set_private_key(key);
    }

    public async getBalance() {
        return this.instance.get_balance();
    }

    public async getPendingTxs() {
        let txs = await this.instance.get_pending_txs();
        return txs.map((tx: any) => Saito.getInstance().factory.createTransaction(tx));
    }

    public async getSlips(): Promise<WalletSlip[]> {
        return this.instance.get_slips().then(slips => {
            return slips.map(slip => new WalletSlip(slip));
        });
    }

    public async addSlips(slips: WalletSlip[]) {
        for (const slip of slips) {
            await this.instance.add_slip(slip.instance);
        }
    }
}

export class WalletSlip extends WasmWrapper<WasmWalletSlip> {
    public static Type: any;

    public constructor(slip?: WasmWalletSlip) {
        if (!slip) {
            slip = new WalletSlip.Type();
        }
        super(slip!);
    }

    public toJson() {
        return {
            utxokey: this.instance.get_utxokey(),
            lc: this.instance.is_lc(),
            spent: this.instance.is_spent(),
            blockId: this.instance.get_block_id(),
            txIndex: this.instance.get_tx_ordinal(),
            slipIndex: this.instance.get_slip_index()
        };
    }

    public copyFrom(json: any) {
        this.instance.set_utxokey(json.utxokey);
        this.instance.set_lc(json.lc);
        this.instance.set_spent(json.spent);
        this.instance.set_block_id(BigInt(json.blockId));
        this.instance.set_tx_ordinal(BigInt(json.txIndex));
        this.instance.set_slip_index(json.slipIndex);
    }
}
