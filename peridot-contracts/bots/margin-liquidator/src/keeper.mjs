import { nativeToScVal, scValToNative } from "@stellar/stellar-sdk";

import { scAddress, scU128, scU64 } from "./stellar.mjs";

const HF_SCALE = 1_000_000n;
const BPS_SCALE = 10_000n;
const EVENT_PAGE_LIMIT = 100;

export function enumTag(value) {
  if (Array.isArray(value) && value.length > 0) return String(value[0]);
  if (typeof value === "string") return value;
  return null;
}

export function chooseLiquidationMinOut(quote, slippageBps) {
  const estimated = BigInt(quote.pool_estimated_out);
  const oracleFloor = BigInt(quote.oracle_min_out);
  if (estimated < oracleFloor) {
    throw new Error(`pool quote ${estimated} is below oracle floor ${oracleFloor}`);
  }
  const quotedFloor = (estimated * (BPS_SCALE - BigInt(slippageBps))) / BPS_SCALE;
  return quotedFloor > oracleFloor ? quotedFloor : oracleFloor;
}

export function applyLifecycleEvents(activeIds, events) {
  const active = new Set([...activeIds].map(String));
  for (const event of events) {
    const name = scValToNative(event.topic[0]);
    const positionId = event.topic[2] ? String(scValToNative(event.topic[2])) : null;
    if (!positionId) continue;
    if (name === "position_created") active.add(positionId);
    if (name === "position_removed") active.delete(positionId);
  }
  return active;
}

export class MarginLiquidationKeeper {
  constructor(config, client, state, save, logger = console) {
    this.config = config;
    this.client = client;
    this.state = state;
    this.save = save;
    this.logger = logger;
  }

  async initialize() {
    if (this.state.initialized) return;
    const latest = await this.client.latestLedger();
    let counter = this.config.bootstrapMaxPositionId;
    if (counter === null) {
      try {
        counter = BigInt(await this.client.read("get_position_counter"));
      } catch (error) {
        throw new Error(
          `cannot bootstrap active positions; upgrade the controller or set BOOTSTRAP_MAX_POSITION_ID: ${error.message}`,
        );
      }
    }

    const active = new Set();
    const failed = [];
    for (let id = 1n; id <= counter; id += 1n) {
      try {
        const position = await this.client.read("get_position", [scU64(id)]);
        if (position !== null && position !== undefined) active.add(id.toString());
      } catch (error) {
        failed.push(id.toString());
        this.logger.warn("position bootstrap read failed", { positionId: id.toString(), error: error.message });
      }
    }
    if (failed.length > 0) {
      throw new Error(`position bootstrap incomplete; failed IDs: ${failed.join(",")}`);
    }
    this.state.activePositionIds = [...active];
    this.state.lastEventLedger = this.config.eventStartLedger
      ? this.config.eventStartLedger - 1
      : latest.sequence;
    this.state.initialized = true;
    await this.save();
    this.logger.info("keeper state initialized", {
      counter: counter.toString(),
      activePositions: active.size,
      lastEventLedger: this.state.lastEventLedger,
    });
  }

  async syncEvents() {
    const latest = await this.client.latestLedger();
    const startLedger = this.state.lastEventLedger + 1;
    if (startLedger > latest.sequence) return;
    const symbol = (name) => nativeToScVal(name, { type: "symbol" }).toXDR("base64");
    const filters = [
      {
        type: "contract",
        contractIds: [this.config.controllerId],
        topics: [[symbol("position_created")], [symbol("position_removed")]],
      },
    ];
    let request = {
      startLedger,
      endLedger: latest.sequence,
      filters,
      limit: EVENT_PAGE_LIMIT,
    };
    let active = new Set(this.state.activePositionIds.map(String));
    let processedThrough = latest.sequence;

    for (;;) {
      const response = await this.client.getEvents(request);
      active = applyLifecycleEvents(active, response.events);
      processedThrough = Math.max(processedThrough, response.latestLedger);
      if (response.events.length < EVENT_PAGE_LIMIT) break;
      if (!response.cursor) throw new Error("event page is full but RPC returned no cursor");
      request = { cursor: response.cursor, filters, limit: EVENT_PAGE_LIMIT };
    }

    this.state.activePositionIds = [...active];
    this.state.lastEventLedger = processedThrough;
    await this.save();
  }

  async runCycle() {
    await this.initialize();
    await this.syncEvents();
    const ids = this.state.activePositionIds.slice(0, this.config.maxPositionsPerCycle);
    for (const id of ids) {
      try {
        await this.processPosition(BigInt(id));
      } catch (error) {
        this.logger.error("position processing failed", { positionId: id, error: error.message });
      }
    }
    const stillActive = new Set(this.state.activePositionIds);
    const processed = new Set(ids);
    this.state.activePositionIds = [
      ...this.state.activePositionIds.filter((id) => !processed.has(id)),
      ...ids.filter((id) => stillActive.has(id)),
    ];
    await this.save();
  }

  async processPosition(positionId) {
    const args = [scU64(positionId)];
    const position = await this.client.read("get_position", args);
    if (position === null || position === undefined) {
      this.state.activePositionIds = this.state.activePositionIds.filter(
        (id) => id !== positionId.toString(),
      );
      return;
    }
    const perps = await this.client.read("get_perps_position", args);
    if (perps === null || perps === undefined) return;

    const status = enumTag(position.status);
    if (status === "Open") {
      const health = BigInt(await this.client.read("get_health_factor", args));
      if (health > HF_SCALE) return;
      this.logger.warn("liquidatable position detected", {
        positionId: positionId.toString(),
        healthFactor: health.toString(),
      });
      await this.client.submit("begin_liquidation_v3", [
        scAddress(this.config.publicKey),
        scU64(positionId),
      ]);
      if (this.config.dryRun) return;
      await this.resumeLiquidation(positionId);
      return;
    }
    if (status === "Liquidated") {
      await this.resumeLiquidation(positionId);
      return;
    }
    if (status === "Closing") {
      const pending = await this.client.read("get_pending_perps_close", args);
      if (pending && BigInt(pending.expires_at) < BigInt(Math.floor(Date.now() / 1_000))) {
        await this.client.submit("expire_close_position_v3", args);
      }
      return;
    }
    if (status === "PendingOpen") await this.recoverExpiredPendingOpen(positionId);
  }

  async resumeLiquidation(positionId) {
    const idArg = scU64(positionId);
    let pending = await this.client.read("get_pending_liquidation", [idArg]);
    if (!pending) {
      this.logger.warn("liquidated position has no pending liquidation", {
        positionId: positionId.toString(),
      });
      return;
    }
    if (!this.canTakeOver(pending)) return;

    if (enumTag(pending.stage) === "Started") {
      const quote = await this.client.read("preview_liquidation_v3", [idArg]);
      const minOut = chooseLiquidationMinOut(quote, this.config.slippageBps);
      await this.client.submit("swap_liquidation_v3", [
        scAddress(this.config.publicKey),
        idArg,
        scU128(minOut),
      ]);
      if (this.config.dryRun) return;
      pending = await this.client.read("get_pending_liquidation", [idArg]);
    }
    if (pending && enumTag(pending.stage) === "CollateralConverted" && this.canTakeOver(pending)) {
      await this.client.submit("finish_liquidation_v3", [
        scAddress(this.config.publicKey),
        idArg,
      ]);
    }
  }

  canTakeOver(pending) {
    if (String(pending.liquidator) === this.config.publicKey) return true;
    const takeoverAfter = BigInt(pending.repay_amount);
    return BigInt(Math.floor(Date.now() / 1_000)) > takeoverAfter;
  }

  async recoverExpiredPendingOpen(positionId) {
    const idArg = scU64(positionId);
    const pending = await this.client.read("get_pending_perps_open", [idArg]);
    const execution = await this.client.read("get_pending_perps_open_execution", [idArg]);
    if (!pending || !execution) return;
    if (BigInt(pending.expires_at) >= BigInt(Math.floor(Date.now() / 1_000))) return;
    await this.client.submit("liquidate_position_v3", [
      scAddress(this.config.publicKey),
      idArg,
    ]);
  }
}
