import type { CoinKey } from "../../api/contract";
import { Button, Flyout, IconButton } from "../../components";
import { useCharacter } from "../../state/CharacterContext";

const COINS: { key: CoinKey; label: string }[] = [
  { key: "pp", label: "Platinum" }, { key: "gp", label: "Gold" }, { key: "ep", label: "Electrum" }, { key: "sp", label: "Silver" }, { key: "cp", label: "Copper" },
];

export function CoinsDialog({ onClose }: { onClose: () => void }) {
  const { snapshot, mutate } = useCharacter();
  if (!snapshot) return null;
  const coins = snapshot.inventory.coins;
  const adjust = (coin: CoinKey, delta: number) => void mutate((api, id) => api.adjustCoins({ characterId: id, coin, delta }));
  return (
    <Flyout title="Coin purse" width="coinage" onClose={onClose} footer={<Button variant="flat" onClick={onClose}>Done</Button>}>
      <div className="stack eq-coins-flyout">
        <p role="status"><strong>Total: {snapshot.inventory.totalGp} gp</strong></p>
        {COINS.map((c) => (
          <div key={c.key} className="coin-row">
            <span style={{ width: "6rem" }}>{c.label}</span>
            <IconButton icon="minus" variant="default" label={`Remove 10 ${c.label}`} onClick={() => adjust(c.key, -10)} />
            <IconButton icon="minus" variant="default" label={`Remove 1 ${c.label}`} onClick={() => adjust(c.key, -1)} />
            <output aria-label={`${c.label} coins`} style={{ minWidth: "3rem", textAlign: "center", fontWeight: 700, color: "var(--gold)" }}>{coins[c.key]}</output>
            <IconButton icon="plus" variant="default" label={`Add 1 ${c.label}`} onClick={() => adjust(c.key, 1)} />
            <IconButton icon="plus" variant="default" label={`Add 10 ${c.label}`} onClick={() => adjust(c.key, 10)} />
          </div>
        ))}
        <hr />
        <div className="row">
          <span className="field-label">Convert everything to</span>
          {COINS.map((c) => <Button key={c.key} small onClick={() => void mutate((api, id) => api.convertCoins({ characterId: id, to: c.key }))}>{c.key}</Button>)}
        </div>
      </div>
    </Flyout>
  );
}
