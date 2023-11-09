export default class WasmWrapper<T> {
  public instance: T;

  constructor(instance: T) {
    this.instance = instance;
  }

  free() {
    // @ts-ignore
    this.instance.free();
  }
}
