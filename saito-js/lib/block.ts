import type {WasmBlock} from "saito-wasm/pkg/node/index";
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
                    avg_total_fees: this.instance.avg_total_fees,
                    avg_fee_per_byte: this.instance.avg_fee_per_byte,
                    avg_nolan_rebroadcast_per_block: this.instance.avg_nolan_rebroadcast_per_block,
                    rebroadcast_hash: this.instance.rebroadcast_hash,
                    rebroadcasts: this.instance.rebroadcasts,
                    difficulty: this.instance.difficulty,
                    burnfee: this.instance.burnfee,
                    total_fees: this.instance.total_fees,
                },
                has_keylist_txs: this.instance.has_keylist_txs,
                graveyard: this.instance.graveyard,
                treasury: this.instance.treasury

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

    public get treasury(): bigint {
        return this.instance.treasury;
    }

    public get graveyard(): bigint {
        return this.instance.graveyard;
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

    public get totalFees(): bigint {
        return this.instance.total_fees;
    }

    public get avgFeePerByte(): bigint {
        return this.instance.avg_fee_per_byte;
    }

    public get avgIncome(): bigint {
        return this.instance.avg_income;
    }

    public get avgNolanRebroadcastPerBlock(): bigint {
        return this.instance.avg_nolan_rebroadcast_per_block;
    }

    public get avgTotalFees(): bigint {
        return this.instance.avg_total_fees;
    }

    public get burnFee(): bigint {
        return this.instance.burnfee;
    }

    public get difficulty(): bigint {
        return this.instance.difficulty;
    }

    public get forceLoaded(): boolean {
        return this.instance.force_loaded;
    }

    public get ftIndex(): number {
        return this.instance.ft_index;
    }

    public get ftNum(): number {
        return this.instance.ft_num;
    }

    public get gtIndex(): number {
        return this.instance.gt_index;
    }

    public get gtNum(): number {
        return this.instance.gt_num;
    }

    public get itIndex(): number {
        return this.instance.it_index;
    }

    public get inLongestChain(): boolean {
        return this.instance.in_longest_chain;
    }

    public get rebroadcastHash(): string {
        return this.instance.rebroadcast_hash;
    }

    public get it_num(): number {
        return this.instance.it_num;
    }

    public get totalRebroadcastFeesNolan(): bigint {
        return this.instance.total_rebroadcast_fees_nolan;
    }

    public get totalRebroadcastNolan(): bigint {
        return this.instance.total_rebroadcast_nolan;
    }

    public get totalRebroadcastSlips(): bigint {
        return this.instance.total_rebroadcast_slips;
    }

    public get totalRebroadcastStakingPayoutsNolan(): bigint {
        return this.instance.total_rebroadcast_staking_payouts_nolan;
    }

    public get totalFeesNew(): bigint {
        return this.instance.total_fees_new;
    }

    public get totalFeesAtr(): bigint {
        return this.instance.total_fees_atr;
    }

    public get avgTotalFeesNew(): bigint {
        return this.instance.avg_total_fees_new;
    }

    public get avgTotalFeesAtr(): bigint {
        return this.instance.avg_total_fees_atr;
    }

    public get totalPayoutRouting(): bigint {
        return this.instance.total_payout_routing;
    }

    public get totalPayoutMining(): bigint {
        return this.instance.total_payout_mining;
    }

    public get totalPayoutTreasury(): bigint {
        return this.instance.total_payout_treasury;
    }

    public get totalPayoutGraveyard(): bigint {
        return this.instance.total_payout_graveyard;
    }

    public get avgPayoutRouting(): bigint {
        return this.instance.avg_payout_routing;
    }

    public get avgPayoutTreasury(): bigint {
        return this.instance.avg_payout_treasury;
    }

    public get avgPayoutGraveyard(): bigint {
        return this.instance.avg_payout_graveyard;
    }

    public get avgPayoutAtr(): bigint {
        return this.instance.avg_payout_atr;
    }

    public get feePerByte(): bigint {
        return this.instance.fee_per_byte;
    }

    public get previousBlockUnpaid(): bigint {
        return this.instance.previous_block_unpaid;
    }

    public get totalWork(): bigint {
        return this.instance.total_work;
    }

    public get hasGoldenTicket(): boolean {
        return this.instance.has_golden_ticket;
    }

    public get hasIssuanceTransaction(): boolean {
        return this.instance.has_issuance_transaction;
    }

    public get issuanceTransactionIndex(): bigint {
        return this.instance.issuance_transaction_index;
    }

    public get hasFeeTransaction(): boolean {
        return this.instance.has_fee_transaction;
    }

    public get hasStakingTransaction(): boolean {
        return this.instance.has_staking_transaction;
    }

    public get goldenTicketIndex(): bigint {
        return this.instance.golden_ticket_index;
    }

    public get feeTransactionIndex(): bigint {
        return this.instance.fee_transaction_index;
    }

}
