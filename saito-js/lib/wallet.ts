import type { WasmWallet } from "saito-wasm/pkg/node/index";
import Saito from "../saito";
import WasmWrapper from "./wasm_wrapper";
import { fromBase58, toBase58 } from "./util";

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
}
