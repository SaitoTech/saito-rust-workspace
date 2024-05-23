import { test } from "@playwright/test";
import SlrNode from "../../src/slr.node";
import { execSync } from "child_process";

let fs = require("fs");
let process = require("process");
let { exec } = require("child_process");

test.skip("issuance file generation @consensus", async ({ page, browserName }, testInfo) => {
  if (browserName !== "chromium") {
    testInfo.skip();
    return;
  }
  testInfo.setTimeout(0);
  let dir = "./temp";
  if (fs.existsSync(dir)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir);
  }

  let node = new SlrNode();
  await node.resetNode();

  await node.startNode();

  // await page.waitForTimeout(10000);

  await node.stopNode();
  // generate some blocks

  // run utxo file generation in SLR

  // check the entries in the generated issuance file
});
