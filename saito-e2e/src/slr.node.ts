import { type Page, type Locator } from "@playwright/test";
import SaitoNode from "./saito_node";

let process = require("process");
let fs = require("fs");

export default class SlrNode extends SaitoNode {
  onResetNode(): Promise<void> {
    let originalDir = process.cwd();

    process.chdir(this.nodeDir);

    let beforeTime = Date.now();
    process.exec("npm reset dev");
    let afterTime = Date.now();

    console.log("resetting the node took : " + (afterTime - beforeTime) + "ms");

    process.chdir(originalDir);
    throw new Error("Method not implemented.");
  }

  onStartNode(): Promise<void> {
    throw new Error("Method not implemented.");
  }

  onStopNode(): Promise<void> {
    throw new Error("Method not implemented.");
  }

  onSetIssuance(issuance: string[]): Promise<void> {
    throw new Error("Method not implemented.");
  }

  constructor() {
    super();
    this.nodeDir = "../../saito-lite-rust";
    if (!fs.existsSync(this.nodeDir)) {
      console.log("SLR Dir : " + this.nodeDir);
      throw new Error("SLR Directory path not valid");
    }
  }
}
