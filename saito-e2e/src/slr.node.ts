import { type Page, type Locator } from "@playwright/test";
import SaitoNode, { NodeConfig } from "./saito_node";
import { ChildProcess, execFile, execSync } from "node:child_process";

import process from "process";
import fs from "fs";

export default class SlrNode extends SaitoNode {
  protected checkRunning(): Promise<boolean> {
    throw new Error("Method not implemented.");
  }
  node: ChildProcess;

  protected async onResetNode(): Promise<void> {
    const beforeTime = Date.now();
    execSync("npm run reset dev", {
      cwd: this.nodeDir,
      // shell: "bash",
    });
    //
    // let buffer = execSync("rm -rf data/blocks", {
    //   cwd: this.nodeDir,
    // });

    // fs.rmSync(this.nodeDir + "/data/blocks", { recursive: true, force: true });
    // fs.mkdirSync(this.nodeDir + "/data/blocks", { recursive: true });

    // console.log("buffer : " + buffer.toString("utf-8"));
    const afterTime = Date.now();

    console.log("resetting the node took : " + (afterTime - beforeTime) + "ms");
  }

  protected async onStartNode(): Promise<void> {
    const beforeTime = Date.now();
    this.node = execFile("npm run dev", [], (error, stdout, stderr) => {
      console.error(error);
      console.log(stdout);
      console.error(stderr);
    });
    const afterTime = Date.now();
    console.log("starting the node took : " + (afterTime - beforeTime) + "ms");
  }

  protected async onStopNode(): Promise<void> {
    const beforeTime = Date.now();
    this.node.kill();
    const afterTime = Date.now();
    console.log("stopping the node took : " + (afterTime - beforeTime) + "ms");
  }

  protected onSetIssuance(issuance: string[]): Promise<void> {
    throw new Error("Method not implemented.");
  }

  constructor(config: NodeConfig) {
    super(config);
    console.log("cwd : " + process.cwd());

  }


}
