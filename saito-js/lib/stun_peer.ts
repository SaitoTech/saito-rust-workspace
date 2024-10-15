import Saito from "../saito";

export class StunManager {

    constructor(private saitoInstance: Saito) {
        this.saitoInstance = saitoInstance
    }

    private stunPeers: Map<bigint, { peerConnection: RTCPeerConnection, publicKey: string }> = new Map();

    public getStunPeers(): Map<bigint, { peerConnection: RTCPeerConnection, publicKey: string }> {
        return this.stunPeers;
    }

    public getStunPeer(index: bigint): { peerConnection: RTCPeerConnection, publicKey: string } | undefined {
        return this.stunPeers.get(index);
    }

    public async addStunPeer(publicKey: string, peerConnection: RTCPeerConnection): Promise<bigint> {
        const peerIndex = await Saito.getLibInstance().get_next_peer_index();
        const dataChannelOptions: RTCDataChannelInit = {
            ordered: true,
            protocol: 'saito',
        };
        const dc = peerConnection.createDataChannel('core-channel', dataChannelOptions);

        //@ts-ignore
        peerConnection.dc = dc;

        return new Promise((resolve, reject) => {
            let timeout: any;
            const cleanup = () => {
                if (timeout) clearTimeout(timeout);
                dc.removeEventListener('open', onOpen);
                dc.removeEventListener('error', onError);
            };

            const onOpen = async () => {
                cleanup();
                // Check if this public key already exists
                let existingPeerIndex = this.findPeerIndexByPublicKey(publicKey);
                if (existingPeerIndex !== null) {
                    console.log(`Replacing existing STUN peer with index: ${existingPeerIndex} for public key: ${publicKey}`);
                    // remove stun peer from local map
                    this.removeStunPeer(existingPeerIndex);
                    // remove stun peer from the network
                    console.log(Saito.getLibInstance(), "lib instance")
                    Saito.getLibInstance().remove_stun_peer(existingPeerIndex)
                }

                this.stunPeers.set(peerIndex, { peerConnection, publicKey });
                console.log(`Data channel opened and STUN peer added with index: ${peerIndex} and public key: ${publicKey}`);
                this.setupDataChannelListeners(dc, peerIndex);
                Saito.getLibInstance().process_stun_peer(peerIndex, publicKey)
                    .then(() => resolve(peerIndex))
                    .catch(reject);
            };

            const onError = (error: Event) => {
                cleanup();
                console.error('Error opening data channel for STUN peer', error);
                reject(new Error('Failed to open data channel'));
            };

            dc.addEventListener('open', onOpen);
            dc.addEventListener('error', onError);

            // Set a timeout in case the connection doesn't establish
            timeout = setTimeout(() => {
                cleanup();
                reject(new Error('Timeout while waiting for data channel to open'));
            }, 30000); // 30 seconds timeout
        });
    }

    private findPeerIndexByPublicKey(publicKey: string): bigint | null {
        for (const [index, peer] of this.stunPeers) {
            if (peer.publicKey === publicKey) {
                return index;
            }
        }
        return null;
    }

    public isStunPeer(index: bigint): boolean {
        return this.stunPeers.has(index);
    }


    private setupDataChannelListeners(dataChannel: RTCDataChannel, peerIndex: bigint) {
        dataChannel.onmessage = (messageEvent) => {
            if (messageEvent.data instanceof ArrayBuffer) {
                console.log('received message from peer', peerIndex)
                const buffer = new Uint8Array(messageEvent.data);
                Saito.getInstance().processMsgBufferFromPeer(buffer, peerIndex);
            } else {
                console.warn('Received unexpected data type from STUN peer', peerIndex, messageEvent);
            }
        };

        dataChannel.onerror = (error: Event) => {
            console.error('Data channel error for STUN peer', peerIndex, error);
            if (error instanceof RTCErrorEvent) {
                console.error('Error name:', error.error.name);
                console.error('Error message:', error.error.message);
            }
            console.log('Data channel state after error:', dataChannel.readyState);
        };

        dataChannel.onclose = () => {
            console.log('Data channel closed for STUN peer', peerIndex);
            this.removeStunPeer(peerIndex);
        };
    }

    private removeStunPeer(peerIndex: bigint) {
        if (this.stunPeers.has(peerIndex)) {
            this.stunPeers.delete(peerIndex);
            console.log(`Removed STUN peer with index: ${peerIndex}`);
        } else {
            console.warn(`Attempt to remove non-existent STUN peer with index: ${peerIndex}`);
        }
    }
}

export default StunManager;