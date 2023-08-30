import SharedMethods from "./shared_methods";
import Transaction from "./lib/transaction";
import Block from "./lib/block";
import Factory from "./lib/factory";
import Peer from "./lib/peer";
import Wallet, {DefaultEmptyPrivateKey} from "./lib/wallet";
import Blockchain from "./lib/blockchain";
import {fromBase58, toBase58} from "./lib/util";

// export enum MessageType {
//     HandshakeChallenge = 1,
//     HandshakeResponse,
//     //HandshakeCompletion,
//     ApplicationMessage = 4,
//     Block,
//     Transaction,
//     BlockchainRequest,
//     BlockHeaderHash,
//     Ping,
//     SPVChain,
//     Services,
//     GhostChain,
//     GhostChainRequest,
//     Result,
//     Error,
//     ApplicationTransaction,
// }

export enum LogLevel {
  Error = 0,
  Warn,
  Info,
  Debug,
  Trace,
}

export default class Saito {
  private static instance: Saito;
  private static libInstance: any;
  sockets: Map<bigint, any> = new Map<bigint, any>();
  nextIndex: bigint = BigInt(0);
  factory = new Factory();
  promises = new Map<number, any>();
  private callbackIndex: number = 1;
  private wallet: Wallet | null = null;
  private blockchain: Blockchain | null = null;
  private static wasmMemory: WebAssembly.Memory | null = null;

  public static async initialize(
    configs: any,
    sharedMethods: SharedMethods,
    factory = new Factory(),
    privateKey: string,
    logLevel: LogLevel
  ) {
    console.log("initializing saito lib");
    Saito.instance = new Saito(factory);

    // @ts-ignore
    globalThis.shared_methods = {
      send_message: (peer_index: bigint, buffer: Uint8Array) => {
        sharedMethods.sendMessage(peer_index, buffer);
      },
      send_message_to_all: (buffer: Uint8Array, exceptions: Array<bigint>) => {
        sharedMethods.sendMessageToAll(buffer, exceptions);
      },
      connect_to_peer: (peer_data: any) => {
        sharedMethods.connectToPeer(peer_data);
      },
      write_value: (key: string, value: Uint8Array) => {
        return sharedMethods.writeValue(key, value);
      },
      read_value: (key: string) => {
        return sharedMethods.readValue(key);
      },
      load_block_file_list: () => {
        return sharedMethods.loadBlockFileList();
      },
      is_existing_file: (key: string) => {
        return sharedMethods.isExistingFile(key);
      },
      remove_value: (key: string) => {
        return sharedMethods.removeValue(key);
      },
      disconnect_from_peer: (peer_index: bigint) => {
        return sharedMethods.disconnectFromPeer(peer_index);
      },
      fetch_block_from_peer: (hash: Uint8Array, peer_index: bigint, url: string) => {
        console.log("fetching block : " + url);
        sharedMethods.fetchBlockFromPeer(url).then((buffer: Uint8Array) => {
          Saito.getLibInstance().process_fetched_block(buffer, hash, peer_index);
        });
      },
      process_api_call: (buffer: Uint8Array, msgIndex: number, peerIndex: bigint) => {
        return sharedMethods.processApiCall(buffer, msgIndex, peerIndex).then(() => {});
      },
      process_api_success: (buffer: Uint8Array, msgIndex: number, peerIndex: bigint) => {
        return sharedMethods.processApiSuccess(buffer, msgIndex, peerIndex);
      },
      process_api_error: (buffer: Uint8Array, msgIndex: number, peerIndex: bigint) => {
        return sharedMethods.processApiError(buffer, msgIndex, peerIndex);
      },
      send_interface_event: (event: string, peerIndex: bigint) => {
        return sharedMethods.sendInterfaceEvent(event, peerIndex);
      },
      send_block_success: (hash: string, blockId: bigint) => {
        return sharedMethods.sendBlockSuccess(hash, blockId);
      },
      save_wallet: (wallet: any) => {
        return sharedMethods.saveWallet(wallet);
      },
      load_wallet: (wallet: any) => {
        return sharedMethods.loadWallet(wallet);
      },
      save_blockchain: (blockchain: any) => {
        return sharedMethods.saveBlockchain(blockchain);
      },
      load_blockchain: (blockchain: any) => {
        return sharedMethods.loadBlockchain(blockchain);
      },
      get_my_services: () => {
        return sharedMethods.getMyServices().instance;
      },
    };
    if (privateKey === "") {
      privateKey = DefaultEmptyPrivateKey;
    }

    let configStr = JSON.stringify(configs);
    await Saito.getLibInstance().initialize(configStr, privateKey, logLevel);

    console.log("saito initialized");
  }

  public start() {
    console.log("starting saito threads");
    let intervalTime = 100;
    Saito.getInstance().call_timed_functions(intervalTime, Date.now() - intervalTime);
  }

  public call_timed_functions(interval: number, lastCalledTime: number) {
    setTimeout(() => {
      let time = Date.now();
      Saito.getLibInstance()
        .process_timer_event(BigInt(time - lastCalledTime))
        .then(() => {
          this.call_timed_functions(interval, time);
        });
    }, interval);
  }

  constructor(factory: Factory) {
    this.factory = factory;
  }

  public static getInstance(): Saito {
    return Saito.instance;
  }

  public static getLibInstance(): any {
    return Saito.libInstance;
  }

  public static setLibInstance(instance: any) {
    Saito.libInstance = instance;
  }

  public static setWasmMemory(memory: any) {
    Saito.wasmMemory = memory;
  }

  public static getWasmMemory(): WebAssembly.Memory | null {
    return Saito.wasmMemory;
  }

  public addNewSocket(socket: any): bigint {
    this.nextIndex++;
    this.sockets.set(this.nextIndex, socket);
    return this.nextIndex;
  }

  public getSocket(index: bigint): any | null {
    return this.sockets.get(index);
  }

  public removeSocket(index: bigint) {
    console.log("Removing socket for : " + index);
    let socket = this.sockets.get(index);
    this.sockets.delete(index);
    socket.close();
  }

  public async initialize(configs: any): Promise<any> {
    return Saito.getLibInstance().initialize(configs);
  }

  public async getLatestBlockHash(): Promise<string> {
    return Saito.getLibInstance().get_latest_block_hash();
  }

  public async getBlock<B extends Block>(blockHash: string): Promise<B | null> {
    // console.assert(!!Saito.libInstance, "wasm lib instance not set");
    // console.log("lib instance : ", Saito.getLibInstance());
    try {
      let block = await Saito.getLibInstance().get_block(blockHash);
      return Saito.getInstance().factory.createBlock(block) as B;
    } catch (error) {
      console.error(error);
      return null;
    }
  }

  // public async getPublicKey(): Promise<string> {
  //     return Saito.getLibInstance().get_public_key();
  // }
  //
  // public async getPrivateKey(): Promise<string> {
  //     return Saito.getLibInstance().get_private_key();
  // }

  public async processNewPeer(index: bigint, peer_config: any): Promise<void> {
    return Saito.getLibInstance().process_new_peer(index, peer_config);
  }

  public async processPeerDisconnection(peer_index: bigint): Promise<void> {
    return Saito.getLibInstance().process_peer_disconnection(peer_index);
  }

  public async processMsgBufferFromPeer(buffer: Uint8Array, peer_index: bigint): Promise<void> {
    return Saito.getLibInstance().process_msg_buffer_from_peer(buffer, peer_index);
  }

  public async processFetchedBlock(
    buffer: Uint8Array,
    hash: Uint8Array,
    peer_index: bigint
  ): Promise<void> {
    return Saito.getLibInstance().process_fetched_block(buffer, hash, peer_index);
  }

  public async processTimerEvent(duration_in_ms: bigint): Promise<void> {
    return Saito.getLibInstance().process_timer_event(duration_in_ms);
  }

  public hash(buffer: Uint8Array): string {
    return Saito.getLibInstance().hash(buffer);
  }

  public signBuffer(buffer: Uint8Array, privateKey: String): string {
    return Saito.getLibInstance().sign_buffer(buffer, privateKey);
  }

  public verifySignature(buffer: Uint8Array, signature: string, publicKey: string): boolean {
    return Saito.getLibInstance().verify_signature(buffer, signature, fromBase58(publicKey));
  }

  public async createTransaction<T extends Transaction>(
    publickey = "",
    amount = BigInt(0),
    fee = BigInt(0),
    force_merge = false
  ): Promise<T> {
    let wasmTx = await Saito.getLibInstance().create_transaction(
      fromBase58(publickey),
      amount,
      fee,
      force_merge
    );
    let tx = Saito.getInstance().factory.createTransaction(wasmTx) as T;
    tx.timestamp = new Date().getTime();
    return tx;
  }

  public async getPeers(): Promise<Array<Peer>> {
    let peers = await Saito.getLibInstance().get_peers();
    return peers.map((peer: any) => {
      return this.factory.createPeer(peer);
    });
  }

  public async getPeer(index: bigint): Promise<Peer | null> {
    let peer = await Saito.getLibInstance().get_peer(index);
    if (!peer) {
      return null;
    }
    return this.factory.createPeer(peer);
  }

  public generatePrivateKey(): string {
    return Saito.getLibInstance().generate_private_key();
  }

  public generatePublicKey(privateKey: string): string {
    let key = Saito.getLibInstance().generate_public_key(privateKey);
    return toBase58(key);
  }

  public async propagateTransaction(tx: Transaction) {
    return Saito.getLibInstance().propagate_transaction(tx.wasmTransaction);
  }

  public async sendApiCall(
    buffer: Uint8Array,
    peerIndex: bigint,
    waitForReply: boolean
  ): Promise<Uint8Array> {
    // console.log("saito.sendApiCall : peer = " + peerIndex + " wait for reply = " + waitForReply);
    let callbackIndex = this.callbackIndex++;
    if (waitForReply) {
      return new Promise((resolve, reject) => {
        this.promises.set(callbackIndex, {
          resolve,
          reject,
        });
        Saito.getLibInstance().send_api_call(buffer, callbackIndex, peerIndex);
      });
    } else {
      return Saito.getLibInstance().send_api_call(buffer, callbackIndex, peerIndex);
    }
  }

  public async sendApiSuccess(msgId: number, buffer: Uint8Array, peerIndex: bigint) {
    return Saito.getLibInstance().send_api_success(buffer, msgId, peerIndex);
  }

  public async sendApiError(msgId: number, buffer: Uint8Array, peerIndex: bigint) {
    return Saito.getLibInstance().send_api_error(buffer, msgId, peerIndex);
  }

  public async sendTransactionWithCallback(
    transaction: Transaction,
    callback?: any,
    peerIndex?: bigint
  ): Promise<any> {
    // TODO : implement retry on fail
    // TODO : stun code goes here probably???
    // console.log(
    //   "saito.sendTransactionWithCallback : peer = " + peerIndex + " sig = " + transaction.signature
    // );
    let buffer = transaction.wasmTransaction.serialize();

    await this.sendApiCall(buffer, peerIndex || BigInt(0), !!callback)
      .then((buffer: Uint8Array) => {
        if (callback) {
          // console.log("sendTransactionWithCallback. buffer length = " + buffer.byteLength);

          let tx = this.factory.createTransaction();
          tx.data = buffer;
          tx.unpackData();
          return callback(tx);
        }
      })
      .catch((error) => {
        console.error(error);
        if (callback) {
          return callback({ err: error.toString() });
        }
      });
  }

  public async sendRequest(
    message: string,
    data: any = "",
    callback?: any,
    peerIndex?: bigint
  ): Promise<any> {
    // console.log("saito.sendRequest : peer = " + peerIndex);
    let wallet = await this.getWallet();
    let publicKey = await wallet.getPublicKey();
    let tx = await this.createTransaction(publicKey, BigInt(0), BigInt(0));
    tx.msg = {
      request: message,
      data: data,
    };
    tx.packData();
    return this.sendTransactionWithCallback(
      tx,
      (tx: Transaction) => {
        if (callback) {
          return callback(tx.msg);
        }
      },
      peerIndex
    );
  }

  public async getWallet() {
    if (!this.wallet) {
      let w = await Saito.getLibInstance().get_wallet();
      this.wallet = this.factory.createWallet(w);
    }
    return this.wallet;
  }

  public async getBlockchain() {
    if (!this.blockchain) {
      let b = await Saito.getLibInstance().get_blockchain();
      this.blockchain = this.factory.createBlockchain(b);
    }
    return this.blockchain;
  }

  public async getMempoolTxs() {
    return Saito.getLibInstance().get_mempool_txs();
  }

  // public async loadWallet() {
  //     return Saito.getLibInstance().load_wallet();
  // }
  //
  // public async saveWallet() {
  //     return Saito.getLibInstance().save_wallet();
  // }
  //
  // public async resetWallet() {
  //     return Saito.getLibInstance().reset_wallet();
  // }
}
