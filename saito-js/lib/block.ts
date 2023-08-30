import type { WasmBlock } from "saito-wasm/pkg/node/index";
import Transaction from "./transaction";
import Saito from "../saito";
import WasmWrapper from "./wasm_wrapper";
import { fromBase58 } from "./util";

export enum BlockType {
  Ghost = 0,
  Header = 1,
  Pruned = 2,
  Full = 3,
}

export default class Block extends WasmWrapper<WasmBlock> {
  public static Type: any;

  constructor(block?: WasmBlock) {
    if (!block) {
      block = new Block.Type();
    }
    super(block!);


  }

  public toJson(): string {

    try {
      return JSON.stringify({
        id: JSON.stringify(this.id),
        hash: this.hash,
        type: JSON.stringify(this.block_type),
        previous_block_hash: this.previousBlockHash,
        transactions: this.transactions.map((tx) => tx.toJson()),
      })
    } catch (error) {
      console.error(error);
    }
    return ""
  }


  public get transactions(): Array<Transaction> {
    try {
      return this.instance.transactions.map((tx) => {
        return Saito.getInstance().factory.createTransaction(tx);
      });
    } catch (error) {
      console.error(error);
      return [];
    }
  }

  public get id(): bigint {
    return this.instance.id;
  }

  public get hash(): string {
    return this.instance.hash;
  }

  public get block_type(): BlockType {
    return this.instance.type;
  }

  public get previousBlockHash(): string {
    return this.instance.previous_block_hash;
  }

  public serialize(): Uint8Array {
    return this.instance.serialize();
  }

  public get file_name() {
    return this.instance.file_name;
  }

  public hasKeylistTxs(keylist: Array<string>): boolean {
    let keys = keylist.map((key) => fromBase58(key));
    return this.instance.has_keylist_txs(keys);
  }

  public generateLiteBlock(keylist: Array<string>): Block {
    let keys = keylist.map((key) => fromBase58(key));
    let block = this.instance.generate_lite_block(keys);
    return Saito.getInstance().factory.createBlock(block);
  }

  public deserialize(buffer: Uint8Array) {
    this.instance.deserialize(buffer);
  }
}
