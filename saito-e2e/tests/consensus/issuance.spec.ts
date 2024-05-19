import { test } from "@playwright/test";

let fs = require("fs");
let process = require("process");
let { exec } = require("child_process");

test("issuance file generation @consensus", async ({ page, browserName }, testInfo) => {
  console.log("browserName : " + browserName);
  
  testInfo.setTimeout(0);
  let dir = "./temp";
  if (fs.existsSync(dir)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir);
  }

  // generate some blocks
  exec();

  // run utxo file generation in SLR

  // check the entries in the generated issuance file
  console.log("cwd = " + process.cwd());
});
