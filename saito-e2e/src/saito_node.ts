import { type Page, type Locator } from "@playwright/test";

export default abstract class SaitoNode {
  private _dataDir: string = "./data";
  private _nodeDir: string = ".";

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
    console.log("starting the node...");
    return this.onStartNode();
  }

  protected abstract onStartNode(): Promise<void>;

  async stopNode() {
    console.log("stopping the node...");
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
    console.log("resetting the node");
    return this.onResetNode();
  }

  protected abstract onResetNode(): Promise<void>;
}
