import { type Page, type Locator } from "@playwright/test";
import SaitoNode from "./saito_node";
import { ChildProcess, execFile, execSync } from "node:child_process";

let process = require("process");
let fs = require("fs");

export default class SlrNode extends SaitoNode {
  node: ChildProcess;

  protected async onResetNode(): Promise<void> {
    let beforeTime = Date.now();
    // let buffer = execSync("npm run reset --script-shell bash", {
    //   cwd: this.nodeDir,
    //   // shell: "bash",
    // });
    //
    // let buffer = execSync("rm -rf data/blocks", {
    //   cwd: this.nodeDir,
    // });

    fs.rmSync(this.nodeDir + "/data/blocks", { recursive: true, force: true });
    fs.mkdirSync(this.nodeDir + "/data/blocks", { recursive: true });

    // console.log("buffer : " + buffer.toString("utf-8"));
    let afterTime = Date.now();

    console.log("resetting the node took : " + (afterTime - beforeTime) + "ms");
  }

  protected async onStartNode(): Promise<void> {
    let beforeTime = Date.now();
    this.node = execFile("npm run dev", [], (error, stdout, stderr) => {
      console.error(error);
      console.log(stdout);
      console.error(stderr);
    });
    let afterTime = Date.now();
    console.log("starting the node took : " + (afterTime - beforeTime) + "ms");
  }

  protected async onStopNode(): Promise<void> {
    let beforeTime = Date.now();
    this.node.kill();
    let afterTime = Date.now();
    console.log("stopping the node took : " + (afterTime - beforeTime) + "ms");
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
