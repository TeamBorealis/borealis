"""Source element, Acolyte background and the Grappler feat (SRD 5.1)."""
import re, sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from lib import *

ev = events()

ATTRIBUTION = ('This work includes material taken from the System Reference Document 5.1 ("SRD 5.1") by Wizards of the Coast LLC '
               'and available at https://dnd.wizards.com/resources/systems-reference-document. The SRD 5.1 is licensed under the '
               'Creative Commons Attribution 4.0 International License available at https://creativecommons.org/licenses/by/4.0/legalcode.')
MODIFICATION_NOTE = ('Modified: the SRD text was structured into JSON content elements (rules, setters and HTML fragments); '
                     'text was reformatted and, in places, condensed for the character sheet summaries.')

def source_element():
    d = element('ID_SRD51_SOURCE', 'SRD 5.1', 'Source',
                '<p><strong>System Reference Document 5.1</strong> (2014 rules), released by Wizards of the Coast LLC under the '
                'Creative Commons Attribution 4.0 International License.</p><p>' + esc_attr(ATTRIBUTION) + '</p><p>' + MODIFICATION_NOTE + '</p>',
                setters=[setter('abbreviation', 'SRD'),
                         setter('url', 'https://dnd.wizards.com/resources/systems-reference-document'),
                         setter('author', 'Wizards of the Coast LLC', abbreviation='WOTC', url='https://dnd.wizards.com'),
                         setter('official', 'true'), setter('release', '20230119'),
                         setter('information', 'Content of the System Reference Document 5.1 only (CC-BY-4.0).',
                                url='https://creativecommons.org/licenses/by/4.0/legalcode')])
    d['source'] = 'Core'
    return d

def esc_attr(s):
    return s.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')

def background():
    i = find_head('Acolyte', 21)
    j = find_head('Feature: Shelter of the Faithful', 18, i)
    k = find_head('Suggested Characteristics', 18, j)
    # description: the paragraphs of the Acolyte section
    desc = to_html(body(i))
    feat_html = to_html(body(j))
    sc_intro = to_html(body(k))
    tables, cur, name = {}, None, None
    m = k + 1
    while ev[m]['k'] != 'H':
        if ev[m]['k'] == 'T':
            t = ev[m]['plain'].strip()
            h = re.match(r'^d\d+ (Personality Trait|Ideal|Bond|Flaw)$', t)
            if h:
                name = h.group(1); tables[name] = []
            elif name:
                n = re.match(r'^(\d+) (.*)$', t)
                if n:
                    tables[name].append([int(n.group(1)), n.group(2)])
                elif tables[name]:
                    tables[name][-1][1] += ' ' + t
        m += 1
    tables = {k_: [{'id': n, 'text': clean(tx)} for n, tx in v] for k_, v in tables.items()}
    lists = []
    for lname, num in (('Personality Trait', 2), ('Ideal', 1), ('Bond', 1), ('Flaw', 1)):
        lists.append({'kind': 'select', 'type': 'List', 'name': lname, **({'number': num} if num > 1 else {}), 'items': tables[lname]})
    rules = [grant('ID_PROFICIENCY_SKILL_INSIGHT', 'Proficiency'), grant('ID_PROFICIENCY_SKILL_RELIGION', 'Proficiency'),
             select('Language', 'Language (Acolyte)', supports='Standard||Exotic', number=2),
             grant('ID_BACKGROUND_FEATURE_SHELTER_OF_THE_FAITHFUL', 'Background Feature')] + lists
    bg = element('ID_BACKGROUND_ACOLYTE', 'Acolyte', 'Background', desc + '<h5>SUGGESTED CHARACTERISTICS</h5>' + sc_intro,
                 rules=rules, setters=[setter('short', 'Insight, Religion, 2 Languages')])
    ft = element('ID_BACKGROUND_FEATURE_SHELTER_OF_THE_FAITHFUL', 'Feature: Shelter of the Faithful', 'Background Feature',
                 feat_html, supports=['Background Feature'], sheet={'descriptions': [{'text': first(feat_html)}]})
    return [bg, ft]

def first(html, n=300):
    t = re.sub(r'<[^>]+>', ' ', html); t = re.sub(r'\s+', ' ', t).strip()
    if len(t) <= n: return t
    cut = t[:n]; k = cut.rfind('. ')
    return cut[:k + 1] if k > 60 else cut

def grappler():
    i = find_head('Grappler', 21)
    b = body(i)
    pre = clean(b[0]['plain'])  # "Prerequisite: Strength 13 or higher"
    prereq = pre.split(':', 1)[1].strip()
    html = to_html(b[1:])
    return element('ID_PHB_FEAT_GRAPPLER', 'Grappler', 'Feat', html, prerequisite=prereq, requirements='[str:13]',
                   sheet={'descriptions': [{'text': 'Advantage on attack rolls against a creature you are grappling. You can use your action to pin a creature you are grappling (you and the creature are both restrained).'}]})

if __name__ == '__main__':
    src = source_element()
    print(write_content('source.json', [src], name='SRD 5.1 Source', description='Source record and CC-BY-4.0 attribution for the SRD 5.1 pack.'))
    print(write_content('backgrounds.json', background(), name='SRD 5.1 Background'))
    print(write_content('feats.json', [grappler()], name='SRD 5.1 Feats'))
