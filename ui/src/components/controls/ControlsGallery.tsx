import { useState } from "react";
import { CommandButton, ComboBox, ChoicesFooter, Dialog, Expander, Flyout, ListBox, MessageBox, MetroProgressBar, NumericUpDown, PickerItem, SelectionPicker, TabControl, TextBox, ToggleSwitch, ToolTip } from ".";

/** Dev-only catalogue of the classic builder control library (route /dev/controls, DEV builds only). */
export function ControlsGallery() {
  const [on, setOn] = useState(true);
  const [qty, setQty] = useState(3);
  const [text, setText] = useState("Gnome");
  const [tab, setTab] = useState("a");
  const [sel, setSel] = useState<string[]>(["b"]);
  const [picked, setPicked] = useState("Rock Gnome");
  const [flyout, setFlyout] = useState(false);
  const [dialog, setDialog] = useState(false);
  const [box, setBox] = useState(false);
  return (
    <div className="page stack-lg">
      <h1>Control library</h1>
      <section className="stack">
        <h3>Buttons</h3>
        <div className="row">
          <CommandButton>Default</CommandButton><CommandButton variant="primary">Primary</CommandButton><CommandButton variant="success">Success</CommandButton>
          <CommandButton variant="warning">Warning</CommandButton><CommandButton variant="danger">Danger</CommandButton><CommandButton variant="clear">Clear</CommandButton>
          <CommandButton variant="flat">Flat</CommandButton><CommandButton icon="plus">With icon</CommandButton><CommandButton disabled>Disabled</CommandButton>
          <ToolTip text="Tooltip text"><CommandButton icon="cog" aria-label="Settings" className="btn-icon" /></ToolTip>
        </div>
      </section>
      <section className="stack" style={{ maxWidth: 420 }}>
        <h3>Inputs</h3>
        <ToggleSwitch checked={on} onChange={setOn} label="Demo switch" />
        <TextBox value={text} onChange={(e) => setText(e.target.value)} onClear={() => setText("")} placeholder="Compendium search" aria-label="Demo text" />
        <TextBox light value="Light field" readOnly aria-label="Light text" />
        <ComboBox aria-label="Demo combo" options={[{ value: "a", label: "Core Rules" }, { value: "b", label: "Supplemental Rules" }]} />
        <NumericUpDown value={qty} onChange={setQty} label="Quantity" compact />
        <MetroProgressBar value={0.6} label="Demo progress" />
        <TabControl tabs={[{ id: "a", label: "Race" }, { id: "b", label: "Class" }, { id: "c", label: "Feats", badge: 2 }]} active={tab} onChange={setTab} label="Demo tabs" idPrefix="demo" />
      </section>
      <section className="stack" style={{ maxWidth: 520 }}>
        <h3>ListBox</h3>
        <ListBox items={["a", "b", "c"]} getKey={(i) => i} selected={sel} onSelect={(i) => setSel([i])} label="Demo list" itemTemplate={(i) => <div style={{ padding: "4px 8px" }}>Item {i}</div>} />
        <h3>Selection picker</h3>
        <SelectionPicker label="Demo picker" footer={<ChoicesFooter remaining={2} total={3} />}>
          <Expander title="Gnome subrace">
            {["Forest Gnome", "Rock Gnome", "Svirfneblin"].map((n) => (
              <PickerItem key={n} name={n} source="Core Rules" selected={picked === n} onSelect={() => setPicked(n)} onClear={() => setPicked("")} />
            ))}
          </Expander>
        </SelectionPicker>
        <div className="row">
          <CommandButton onClick={() => setFlyout(true)}>Open flyout</CommandButton>
          <CommandButton onClick={() => setDialog(true)}>Open dialog</CommandButton>
          <CommandButton onClick={() => setBox(true)}>Open message box</CommandButton>
        </div>
      </section>
      {flyout && <Flyout title="New character" onClose={() => setFlyout(false)} footer={<CommandButton variant="flat" onClick={() => setFlyout(false)}>Cancel</CommandButton>}><div className="field"><label htmlFor="g-name">Name</label><TextBox id="g-name" light value="Zook" readOnly /></div></Flyout>}
      {dialog && <Dialog title="Dialog" onClose={() => setDialog(false)}>Dialog body</Dialog>}
      {box && <MessageBox title="Delete character" message="Delete this character permanently?" buttons={[{ id: "ok", label: "Delete", variant: "danger" }, { id: "no", label: "Cancel" }]} onResult={() => setBox(false)} />}
    </div>
  );
}
