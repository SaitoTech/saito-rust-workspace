import { test } from "@playwright/test";
import SlrNode from "../../src/slr.node";
import { NodeSet, NodeSetConfig } from "../../src/node_set";
import { NodeConfig, NodeType } from "../../src/saito_node";

test.describe("nodes should sync correctly", () => {
    let nodeSetup: NodeSet;
    test.beforeAll(async () => {
        const configSet = new NodeSetConfig();
        configSet.mainNodeIndex = 0;
        configSet.basePort = 42000;
        configSet.parentDir = "node-sync-test";
        configSet.genesisPeriod = BigInt(100);

        let config = new NodeConfig();
        config.name = "main";
        config.isGenesis = true;
        config.nodeType = NodeType.SLR;
        config.port = 1;
        config.originalCodeLocation = "../saito-lite-rust";
        configSet.nodeConfigs.push(config);

        config = new NodeConfig();
        config.name = "peer";
        config.isGenesis = true;
        config.nodeType = NodeType.SLR;
        config.port = 2;
        config.peers = ["main"];
        config.originalCodeLocation = "../saito-lite-rust";
        configSet.nodeConfigs.push(config);

        nodeSetup = new NodeSet(configSet);
        await nodeSetup.bootstrap();

        await nodeSetup.startNodes();

    });
    test.afterAll(async () => {
        await nodeSetup.stopNodes();

    });

    test("sync peer after peer has 10 blocks", async ({ request }) => {
        console.log("running the test");

    });
});

test.skip("issuance file generation @consensus", async ({ page, browserName, request }, testInfo) => {
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
