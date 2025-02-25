import SaitoNode, { NodeConfig, NodeType } from "./saito_node";
import path from "path";
import fs from "fs/promises";
import SlrNode from "./slr.node";
const { exec } = require("child_process");

const TEST_DIR = "temp_test_directory";

export class NodeSetConfig {
    nodeConfigs: NodeConfig[] = [];
    mainNodeIndex: number = 0;
    issuance: string[] = [];
    genesisPeriod: bigint = BigInt(100);
    parentDir: string = "";
    basePort: number = 42000;
}

export class NodeSet {
    nodes: SaitoNode[] = [];
    config: NodeSetConfig;
    constructor(configSet: NodeSetConfig) {
        this.config = configSet;
    }

    async bootstrap() {
        if (this.config.parentDir == "" || this.config.parentDir == "/") {
            throw new Error("Parent directory not set");
        }
        for (const config of this.config.nodeConfigs) {
            const dir = path.join(TEST_DIR, this.config.parentDir, config.name);
            console.assert(config.port < 100);
            console.assert(this.config.basePort > 10000);
            const port = this.config.basePort + config.port;
            const node = await Bootstrapper.bootstrap({ ...config, dir: dir, port: port });
            this.nodes.push(node);
        }
    }
    async startNodes() {
        console.log("starting node set");
        return Promise.all(this.nodes.map((node) => { return node.startNode(); }));
    }
    async stopNodes() {
        return Promise.all(this.nodes.map((node) => { return node.stopNode(); }));
    }
    getNode(name:string):SaitoNode |undefined{
        return this.nodes.find((node) => node.name === name);
    }
}

export class Bootstrapper {
    static async bootstrap(config: NodeConfig): Promise<SaitoNode> {
        if (config.nodeType === NodeType.SLR) {
            return new SlrBootstrapper().bootstrap(config);
        } else if (config.nodeType === NodeType.RUST) {
            return new RustBootstrapper().bootstrap(config);
        }
        throw new Error("Unknown node type : " + config.nodeType);
    }
}

class NodeBootstrapper {
    repoName: string = "";
    dir: string = "";
    config: NodeConfig;

    async bootstrap(config: NodeConfig): Promise<SaitoNode> {
        this.config = config;
        this.dir = config.dir;
        if (this.dir == "" || this.dir == "/") {
            throw new Error("directory path not set");
        }
        await fs.mkdir(this.dir, { recursive: true });
        return await this.onBootstrap();
    }
    protected async onBootstrap(): Promise<SaitoNode> {
        throw new Error("Not Implemented");
    }
    async isDirEmpty(dir: string) {
        const dirPath = path.resolve(dir);
        const files = await fs.readdir(dirPath);
        console.log(`dir ${dir} is not empty. it has ${files.length} files inside`);
        // files.forEach(element => {
        //     console.log("file : " + element);
        // });
        return files.length === 0;
    }
    async fileExists(filename: string) {
        try {
            const filepath = path.join(this.dir, filename);
            await fs.access(filepath);
            return true;
        } catch {
            return false;
        }
    }
    async cleanDir(dir: string) {
        console.log(`cleaning dir : ${dir}`);
        if (!dir.includes(TEST_DIR)) {
            throw new Error(`dir : ${dir} is not inside test folder`);
        }
        const files = await fs.readdir(dir);
        for (const file of files) {
            const filePath = path.join(dir, file);
            const stat = await fs.lstat(filePath);
            if (stat.isDirectory()) {
                console.log("deleting : " + filePath);
                await fs.rm(filePath, { recursive: true, force: true });
            } else {
                console.log("unlinking : " + filePath);
                await fs.unlink(filePath);
            }
        }
    }


    
}

class RustBootstrapper extends NodeBootstrapper {
    constructor() {
        super();
        this.repoName = "";
    }
    async onBootstrap(): Promise<SaitoNode> {
        // clone the repo

        // install required dependencies
        // build the saito-rust binary
        // copy the binary into the correct directory
        throw new Error("Method not implemented.");
    }

}
class SlrBootstrapper extends NodeBootstrapper {
    constructor() {
        super();
        this.repoName = "git@github.com:SaitoTech/saito-lite-rust.git";
    }
    async onBootstrap(): Promise<SaitoNode> {
        // clone the repo
        if (await this.isDirEmpty(this.dir)) {
            if (!this.config.originalCodeLocation) {
                throw new Error("original code location is not set");
            }
            await SaitoNode.runCommand(`rsync -a --exclude='nettest' --exclude='node_modules' --exclude='.git' ${this.config.originalCodeLocation}/ ${this.dir}`,this.dir);
        } else if (!await this.fileExists("README.md")) {
            if (!this.config.originalCodeLocation) {
                throw new Error("original code location is not set");
            }
            await this.cleanDir(this.dir);
            await SaitoNode.runCommand(`rsync -a --exclude='nettest' --exclude='node_modules' --exclude='.git' ${this.config.originalCodeLocation}/ ${this.dir}`,this.dir);
        }
        // install required dependencies
        await SaitoNode.runCommand("npm install",this.dir);

        // compile the project
        await SaitoNode.runCommand("npm run compile",this.dir);

        // run "npm run reset dev"
        await SaitoNode.runCommand("npm run reset dev",this.dir);

        // configurations
        // throw new Error("Method not implemented.");

        const node = new SlrNode(this.config);
        return node;
    }
    async resetProject() {
        await new Promise<void>((resolve, reject) => {
            exec("npm run reset dev", { cwd: this.dir }, (error, stdout, stderr) => {
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
    async installDependencies() {
        await new Promise<void>((resolve, reject) => {
            exec("npm install", { cwd: this.dir }, (error, stdout, stderr) => {
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
