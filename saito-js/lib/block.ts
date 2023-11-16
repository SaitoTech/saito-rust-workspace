import type { WasmBlock } from "saito-wasm/pkg/node/index";
import Transaction from "./transaction";
import Saito from "../saito";
import WasmWrapper from "./wasm_wrapper";

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
        cv: this.instance.cv,
        it_index: this.instance.it_index,
        fee_transaction: this.instance.fee_transaction,
        it_num: this.instance.it_num,
        block_payout: this.instance.block_payout,
        ft_num: this.instance.ft_num,
        ft_index: this.instance.ft_index,
        gt_index: this.instance.gt_index,
        total_fees: this.instance.total_fees,
        expected_difficulty: this.instance.expected_difficulty,
        avg_atr_income: this.instance.avg_atr_income,
        avg_atr_variance: this.instance.avg_atr_variance,
        total_rebroadcast_slips: this.instance.total_rebroadcast_slips,
        total_rebroadcast_nolan: this.instance.total_rebroadcast_nolan,
        total_rebroadcast_fees_nolan: this.instance.total_rebroadcast_fees_nolan,
        total_rebroadcast_staking_payouts_nolan: this.instance.total_rebroadcast_staking_payouts_nolan,
        rebroadcast_hash: this.instance.rebroadcast_hash,
        nolan_falling_off_chain: this.instance.nolan_falling_off_chain,
        staking_treasury: this.instance.staking_treasury,
        avg_income: this.instance.avg_income,
        avg_variance: this.instance.avg_variance,
        timestamp: this.instance.timestamp,
        creator: this.instance.creator,
        file_name: this.instance.file_name,
        has_keylist_txs: this.instance.has_keylist_txs


      });
    } catch (error) {
      console.error(error);
    }
    return "";
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
    return this.instance.has_keylist_txs(keylist);
  }

  public generateLiteBlock(keylist: Array<string>): Block {
    let block = this.instance.generate_lite_block(keylist);
    console.assert(block.hash === this.hash, `this block's hash : ${this.hash} does not match with generated lite block's hash : ${block.hash}`);

    return Saito.getInstance().factory.createBlock(block);
  }

  public deserialize(buffer: Uint8Array) {
    this.instance.deserialize(buffer);
  }
}
