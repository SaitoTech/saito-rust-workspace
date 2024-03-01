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
        id: this.id,
        hash: this.hash,
        type: JSON.stringify(this.block_type),
        previous_block_hash: this.previousBlockHash,
        transactions: this.transactions.map(tx => tx.toJson()),
        nolan_falling_off_chain: this.instance.nolan_falling_off_chain,
        timestamp: this.instance.timestamp,
        creator: this.instance.creator,
        file_name: this.instance.file_name,
        cv: {
          fee_transaction: this.fee_transaction,
          ft_num: this.instance.ft_num,
          ft_index: this.instance.ft_index,
          gt_index: this.instance.gt_index,
          it_index: this.instance.it_index,
          it_num: this.instance.it_num,
          total_rebroadcast_slips: this.instance.total_rebroadcast_slips,
          total_rebroadcast_nolan: this.instance.total_rebroadcast_nolan,
          total_rebroadcast_fees_nolan: this.instance.total_rebroadcast_fees_nolan,
          total_rebroadcast_staking_payouts_nolan: this.instance.total_rebroadcast_staking_payouts_nolan,
          staking_payout: this.instance.staking_payout,
          avg_fee_per_byte: this.instance.avg_fee_per_byte,
          avg_nolan_rebroadcast_per_block: this.instance.avg_nolan_rebroadcast_per_block,
          rebroadcast_hash: this.instance.rebroadcast_hash,
          avg_income: this.instance.avg_income,
          rebroadcasts: this.instance.rebroadcasts,
          expected_difficulty: this.instance.expected_difficulty,
          expected_burnfee: this.instance.expected_burnfee,
          total_fees: this.instance.total_fees,
        },
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

  public get fee_transaction(): Transaction {
    return Saito.getInstance().factory.createTransaction(this.instance.fee_transaction);
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
