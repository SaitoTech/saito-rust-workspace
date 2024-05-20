import { type Page, type Locator } from "@playwright/test";
import SaitoNode from "./saito_node";
import { execFile, execSync } from "node:child_process";

let process = require("process");
let fs = require("fs");

export default class SlrNode extends SaitoNode {
  protected async onResetNode(): Promise<void> {
    // let originalDir = process.cwd();

    // process.chdir(this.nodeDir);

    let beforeTime = Date.now();
    let buffer = execSync("npm run reset --verbose --script-shell bash", {
      cwd: this.nodeDir,
    });

    console.log("buffer : " + buffer.toString("utf-8"));
    let afterTime = Date.now();

    console.log("resetting the node took : " + (afterTime - beforeTime) + "ms");

    // process.chdir(originalDir);
    // throw new Error("Method not implemented.");
  }

  protected onStartNode(): Promise<void> {
    throw new Error("Method not implemented.");
  }

  protected onStopNode(): Promise<void> {
    throw new Error("Method not implemented.");
  }

  protected onSetIssuance(issuance: string[]): Promise<void> {
    throw new Error("Method not implemented.");
  }

  constructor() {
    super();
    console.log("cwd : " + process.cwd());

    this.nodeDir = "../../saito-lite-rust";
    if (!fs.existsSync(this.nodeDir)) {
      console.log("SLR Dir : " + this.nodeDir);
      throw new Error("SLR Directory path not valid");
    }
  }
}
