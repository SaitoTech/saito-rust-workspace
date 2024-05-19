import { type Page, type Locator } from "@playwright/test";

export default abstract class SaitoNode {
  private _dataDir: string = "./data";

  public set dataDir(dir: string) {
    this._dataDir = dir;
  }

  public get dataDir(): string {
    return this._dataDir;
  }

  async startNode() {
    return this.onStartNode();
  }

  abstract onStartNode(): Promise<void>;

  async stopNode() {
    return this.onStopNode();
  }

  abstract onStopNode(): Promise<void>;

  async cleanDataFolder() {
    throw new Error("NotImplemented");
  }

  async setIssuance(issuance: string[]) {
    return this.onSetIssuance(issuance);
  }

  abstract onSetIssuance(issuance: string[]): Promise<void>;
}
