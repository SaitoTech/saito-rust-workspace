export enum NodeType {
  RUST = "rust",
  SLR = "slr",
}

export class NodeConfig {
  peers: string[] = [];
  port: number = 0;
  dir: string = "";
  name: string = "";
  nodeType: NodeType = NodeType.SLR;
  isGenesis: boolean = false;
  originalCodeLocation: string = "";
}
export default abstract class SaitoNode {
  private _dataDir: string = "./data";
  private _nodeDir: string = ".";
  private _config: NodeConfig;
  private _index: number = -1;

  constructor(config: NodeConfig) {
    this._config = config;
    this._nodeDir = config.dir;
  }

  public set dataDir(dir: string) {
    this._dataDir = dir;
  }

  public get dataDir(): string {
    return this._dataDir;
  }

  public set nodeDir(dir: string) {
    this._nodeDir = dir;
  }

  public get nodeDir(): string {
    return this._nodeDir;
  }

  async startNode() {
    console.log(`starting the node : ${this._index}...`);
    return this.onStartNode();
  }

  protected abstract onStartNode(): Promise<void>;

  async stopNode() {
    console.log(`stopping the node : ${this._index}...`);
    return this.onStopNode();
  }

  protected abstract onStopNode(): Promise<void>;

  async cleanDataFolder() {
    throw new Error("NotImplemented");
  }

  async setIssuance(issuance: string[]) {
    return this.onSetIssuance(issuance);
  }

  protected abstract onSetIssuance(issuance: string[]): Promise<void>;

  async resetNode() {
    console.log(`resetting the node : ${this._index}`);
    return this.onResetNode();
  }

  protected abstract onResetNode(): Promise<void>;

  async isRunning(): Promise<boolean> {
    return this.checkRunning();
  }

  protected abstract checkRunning(): Promise<boolean>;

  public async getLatestBlockHash(): Promise<string> { return ""; }
  public async getPeers() { }
  public async sendMessage() { }

  public async deploy() {

  }
}

// API requirements.
// to support E2E testing, a running node need to be able to (only enabled from an environment variable):
// - provide the status of the node
// - provide the latest block hash
// - provide the list of peers
// - provide the latest block picture
// - TODO realize this list
// if any of these endpoints are open we should be able to say the node has started
