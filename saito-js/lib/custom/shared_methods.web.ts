import Saito from "../../saito";

import CustomSharedMethods from "./custom_shared_methods";

export default class WebSharedMethods extends CustomSharedMethods {
  connectToPeer(peerData: any): void {
    let protocol = "ws";
    if (peerData.protocol === "https") {
      protocol = "wss";
    }
    let url = protocol + "://" + peerData.host + ":" + peerData.port + "/wsopen";

    try {
      console.log("connecting to " + url + "....");
      let socket = new WebSocket(url);
      socket.binaryType = "arraybuffer";
      let index = Saito.getInstance().addNewSocket(socket);

      socket.onmessage = (event: MessageEvent) => {
        try {
          Saito.getLibInstance().process_msg_buffer_from_peer(new Uint8Array(event.data), index);
        } catch (error) {
          console.error(error);
        }
      };

      socket.onopen = () => {
        try {
          Saito.getLibInstance().process_new_peer(index, peerData);
        } catch (error) {
          console.error(error);
        }
      };
      socket.onclose = () => {
        try {
          console.log("socket.onclose : " + index);
          Saito.getLibInstance().process_peer_disconnection(index);
        } catch (error) {
          console.error(error);
        }
      };

      console.log("connected to : " + url + " with peer index : " + index);
    } catch (e) {
      console.error(e);
    }
  }

  disconnectFromPeer(peerIndex: bigint): void {
    console.log("disconnect from peer : " + peerIndex);
    Saito.getInstance().removeSocket(peerIndex);
  }

  fetchBlockFromPeer(url: string): Promise<Uint8Array> {
    return fetch(url)
      .then((res: any) => {
        return res.arrayBuffer();
      })
      .then((buffer: ArrayBuffer) => {
        return new Uint8Array(buffer);
      });
  }

  isExistingFile(key: string): boolean {
    try {
      return !!localStorage.getItem(key);
    } catch (error) {
      console.error(error);
      return false;
    }
  }

  loadBlockFileList(): Array<string> {
    try {
      // console.log("loading block file list...");
      // let files = Object.keys(localStorage);
      // console.log("files : ", files);
      // return files;
      return [];
    } catch (e) {
      console.error(e);
      return [];
    }
  }

  readValue(key: string): Uint8Array {
    try {
      let data = localStorage.getItem(key);
      if (!data) {
        console.log("item not found for key : " + key);
        return new Uint8Array();
      }
      let buffer = Buffer.from(data, "base64");
      return new Uint8Array(buffer);
    } catch (error) {
      console.error(error);
    }
    return new Uint8Array();
  }

  removeValue(key: string): void {
    try {
      localStorage.removeItem(key);
    } catch (e) {
      console.error(e);
    }
  }

  sendMessage(peerIndex: bigint, buffer: Uint8Array): void {
    // console.debug("sending message to peer : " + peerIndex + " with size : " + buffer.byteLength);
    let socket = Saito.getInstance().getSocket(peerIndex);
    if (socket) {
      socket.send(buffer);
    }
  }

  sendMessageToAll(buffer: Uint8Array, exceptions: Array<bigint>): void {
    // console.debug("sending message to  all with size : " + buffer.byteLength);
    Saito.getInstance().sockets.forEach((socket, key) => {
      if (exceptions.includes(key)) {
        return;
      }
      socket.send(buffer);
    });
  }

  writeValue(key: string, value: Uint8Array): void {
    try {
      localStorage.setItem(key, Buffer.from(value).toString("base64"));
    } catch (error) {
      console.error(error);
    }
  }

  sendInterfaceEvent(event: String, peerIndex: bigint) {
    throw new Error("Method not implemented.");
  }
}
