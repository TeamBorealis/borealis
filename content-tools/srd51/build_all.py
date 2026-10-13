"""Regenerate the whole srd-5.1 pack: python3 -I build_all.py [--prepare]"""
import os, subprocess, sys
HERE = os.path.dirname(os.path.abspath(__file__))
STEPS = ['build_spells.py', 'build_items.py', 'build_races.py', 'build_misc.py', 'build_conditions.py',
         'cls_fighter.py', 'cls_barbarian.py', 'cls_bard.py', 'cls_cleric.py', 'cls_druid.py', 'cls_monk.py',
         'cls_paladin.py', 'cls_ranger.py', 'cls_rogue.py', 'cls_sorcerer.py', 'cls_warlock.py', 'cls_wizard.py',
         'build_pack.py', 'build_coverage.py']

def main():
    if '--prepare' in sys.argv:
        subprocess.run([sys.executable, '-I', os.path.join(HERE, 'prepare.py')], check=True)
    for s in STEPS:
        print('==', s, flush=True)
        subprocess.run([sys.executable, '-I', os.path.join(HERE, s)], check=True)

if __name__ == '__main__':
    main()
