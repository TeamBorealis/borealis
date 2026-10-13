"""Conditions of SRD 5.1 Appendix PH-A (rules/conditions.json), with maps_to links to the SRD 5.2.1 conditions."""
import os, sys, html
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import element, write_content

def ul(*items):
    return '<ul>' + ''.join(f'<li>{html.escape(i, quote=False)}</li>' for i in items) + '</ul>'

EXHAUSTION_LEVELS = [
    ('1', 'Disadvantage on ability checks'), ('2', 'Speed halved'),
    ('3', 'Disadvantage on attack rolls and saving throws'), ('4', 'Hit point maximum halved'),
    ('5', 'Speed reduced to 0'), ('6', 'Death')]

EXHAUSTION = (
    '<p>Some special abilities and environmental hazards, such as starvation and the long-term effects of freezing or '
    'scorching temperatures, can lead to a special condition called exhaustion. Exhaustion is measured in six levels. '
    'An effect can give a creature one or more levels of exhaustion, as specified in the effect’s description.</p>'
    '<table><thead><tr><th>Level</th><th>Effect</th></tr></thead><tbody>'
    + ''.join(f'<tr><td>{n}</td><td>{e}</td></tr>' for n, e in EXHAUSTION_LEVELS) + '</tbody></table>'
    '<p>If an already exhausted creature suffers another effect that causes exhaustion, its current level of exhaustion '
    'increases by the amount specified in the effect’s description.</p>'
    '<p>A creature suffers the effect of its current level of exhaustion as well as all lower levels. For example, a '
    'creature suffering level 2 exhaustion has its speed halved and has disadvantage on ability checks.</p>'
    '<p>An effect that removes exhaustion reduces its level as specified in the effect’s description, with all exhaustion '
    'effects ending if a creature’s exhaustion level is reduced below 1.</p>'
    '<p>Finishing a long rest reduces a creature’s exhaustion level by 1, provided that the creature has also ingested '
    'some food and drink.</p>')

CONDITIONS = [
    ('Blinded', ul('A blinded creature can’t see and automatically fails any ability check that requires sight.',
                   'Attack rolls against the creature have advantage, and the creature’s attack rolls have disadvantage.')),
    ('Charmed', ul('A charmed creature can’t attack the charmer or target the charmer with harmful abilities or magical effects.',
                   'The charmer has advantage on any ability check to interact socially with the creature.')),
    ('Deafened', ul('A deafened creature can’t hear and automatically fails any ability check that requires hearing.')),
    ('Exhaustion', EXHAUSTION),
    ('Frightened', ul('A frightened creature has disadvantage on ability checks and attack rolls while the source of its fear is within line of sight.',
                      'The creature can’t willingly move closer to the source of its fear.')),
    ('Grappled', ul('A grappled creature’s speed becomes 0, and it can’t benefit from any bonus to its speed.',
                    'The condition ends if the grappler is incapacitated (see the condition).',
                    'The condition also ends if an effect removes the grappled creature from the reach of the grappler or grappling effect, such as when a creature is hurled away by the thunderwave spell.')),
    ('Incapacitated', ul('An incapacitated creature can’t take actions or reactions.')),
    ('Invisible', ul('An invisible creature is impossible to see without the aid of magic or a special sense. For the purpose of hiding, the creature is heavily obscured. The creature’s location can be detected by any noise it makes or any tracks it leaves.',
                     'Attack rolls against the creature have disadvantage, and the creature’s attack rolls have advantage.')),
    ('Paralyzed', ul('A paralyzed creature is incapacitated (see the condition) and can’t move or speak.',
                     'The creature automatically fails Strength and Dexterity saving throws.',
                     'Attack rolls against the creature have advantage.',
                     'Any attack that hits the creature is a critical hit if the attacker is within 5 feet of the creature.')),
    ('Petrified', ul('A petrified creature is transformed, along with any nonmagical object it is wearing or carrying, into a solid inanimate substance (usually stone). Its weight increases by a factor of ten, and it ceases aging.',
                     'The creature is incapacitated (see the condition), can’t move or speak, and is unaware of its surroundings.',
                     'Attack rolls against the creature have advantage.',
                     'The creature automatically fails Strength and Dexterity saving throws.',
                     'The creature has resistance to all damage.',
                     'The creature is immune to poison and disease, although a poison or disease already in its system is suspended, not neutralized.')),
    ('Poisoned', ul('A poisoned creature has disadvantage on attack rolls and ability checks.')),
    ('Prone', ul('A prone creature’s only movement option is to crawl, unless it stands up and thereby ends the condition.',
                 'The creature has disadvantage on attack rolls.',
                 'An attack roll against the creature has advantage if the attacker is within 5 feet of the creature. Otherwise, the attack roll has disadvantage.')),
    ('Restrained', ul('A restrained creature’s speed becomes 0, and it can’t benefit from any bonus to its speed.',
                      'Attack rolls against the creature have advantage, and the creature’s attack rolls have disadvantage.',
                      'The creature has disadvantage on Dexterity saving throws.')),
    ('Stunned', ul('A stunned creature is incapacitated (see the condition), can’t move, and can speak only falteringly.',
                   'The creature automatically fails Strength and Dexterity saving throws.',
                   'Attack rolls against the creature have advantage.')),
    ('Unconscious', ul('An unconscious creature is incapacitated (see the condition), can’t move or speak, and is unaware of its surroundings.',
                       'The creature drops whatever it’s holding and falls prone.',
                       'The creature automatically fails Strength and Dexterity saving throws.',
                       'Attack rolls against the creature have advantage.',
                       'Any attack that hits the creature is a critical hit if the attacker is within 5 feet of the creature.')),
]

def main():
    els = []
    for name, desc in CONDITIONS:
        e = element(f'ID_SRD51_CONDITION_{name.upper()}', name, 'Condition', desc)
        e['ruleset'] = 'dnd2014'
        e['maps_to'] = {'dnd2024': f'ID_2024_CONDITION_{name.upper()}', 'kind': 'equivalent'}
        els.append(e)
    write_content('rules/conditions.json', els, 'SRD 5.1 Conditions',
                  'The game conditions of SRD 5.1 Appendix PH-A, linked to their SRD 5.2.1 counterparts.')
    print(len(els), 'conditions')

if __name__ == '__main__':
    main()
