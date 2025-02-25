const { exec } = require("child_process");

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
  host: string = "localhost";
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
  public get name(): string {
    return this._config.name;
  }

  async startNode() {
    console.log(`starting the node : ${this._index}...`);

    await this.onStartNode();

    for (let i = 0; i < 30; i++) {
      const running = await this.isRunning();
      if (running) {
        console.log(`node started : ${this._index}`);
        break;
      } else {
        console.log("waiting for node : " + this.name + " to start");
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
    }
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
    try {
      await this.fetchValueFromNode("status");
      return true;
    } catch (error) {
      console.error(error.message);
      return false;
    }
  }

  public async getLatestBlock(): Promise<{
    hash: string;
    id: number;
    previous_block_hash: string;
  }> {
    const result = await this.fetchValueFromNode("block/latest");
    return result as { hash: string; id: number; previous_block_hash: string };
  }
  public async getPeers(): Promise<{ peers: string[] }> {
    return (await this.fetchValueFromNode("peers/all")) as { peers: string[] };
  }
  public async transferFunds(amount: bigint, to: string) {
    return await this.fetchValueFromNode(`transfer/${to}/${amount}`);
  }
  public async sendMessage() {}

  public async fetchValueFromNode(path: string): Promise<unknown> {
    return fetch(this.getUrl(path)).then((res) => res.json());
  }
  public getUrl(path: string): string {
    return `http://${this._config.host}:${this._config.port}/test-api/${path}`;
  }
  public static async runCommand(command: string, cwd:string) {
    await new Promise<void>((resolve, reject) => {
        console.log("running command : " + command);
        exec(command, { cwd: cwd }, (error, stdout, stderr) => {
            if (error) {
                console.error(`Error running reset command: ${error.message}`);
                reject(error);
                return;
            }
            if (stderr) {
                console.error(`Command stderr: ${stderr}`);
            }
            console.log(`Command stdout: ${stdout}`);
            resolve();
        });
    });
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
