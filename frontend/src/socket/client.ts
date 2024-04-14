export default class WebSocketClient {
  private url: string;
  private socket: WebSocket | null;
  private onMessage: MessageHandler;
  private requested: {
    [key: string]: {
      resolve: (value: any) => void;
      reject: (reason: any) => void;
    };
  };

  constructor(url: string, onMessage: MessageHandler) {
    this.url = url;
    this.socket = null;
    this.onMessage = onMessage;
    this.requested = {};
  }

  public connect(callback: () => void) {
    this.socket = new WebSocket(this.url);
    this.socket.onmessage = this.message;
    this.socket.onopen = callback;
  }

  public disconnect() {
    if (this.socket) {
      console.log("disconnect websocket");
      this.socket.close();
      this.socket = null;
    }
  }

  public send(data: any) {
    if (this.socket) {
      this.socket.send(JSON.stringify(data));
    }
  }

  public async invoke(data: any, action: string): Promise<any> {
    const self = this;
    return new Promise((resolve, reject) => {
      self.requested[action] = { resolve, reject };
      self.send(data);
    });
  }

  // When if this is a function, `this` scope will be WebSocket object.
  private message = (e: MessageEvent) => {
    const message: Message = JSON.parse(e.data);
    if (message.action && this.requested[message.action]) {
      this.requested[message.action].resolve(e);
      delete this.requested[message.action];
      return;
    }
    this.onMessage(e);
  };
}

type Message = {
  action: string;
};

export type MessageHandler = (e: MessageEvent) => void;
