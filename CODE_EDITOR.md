# Code Editor

`code_editor/` remplace l'ancien domaine `frontend/`.

Ce domaine regroupe tout ce qui concerne les extensions utilisables depuis des éditeurs de code, les intégrations propres aux différents éditeurs et la liaison entre les deux.

L'architecture repose sur trois ensembles distincts :

```text
code_editor/
├── editors/
├── extensions/
└── link_between_extensions_and_editors/
```

La relation fondamentale est :

```text
extensions/
    ↓
link_between_extensions_and_editors/
    ↓
editors/
```

Une extension demande une opération liée à l'éditeur.

La couche `link_between_extensions_and_editors/` transmet cette demande vers l'intégration de l'éditeur actif.

L'intégration de l'éditeur réalise concrètement l'opération et renvoie un résultat normalisé.

L'extension ne connaît ni l'éditeur, ni son langage d'implémentation, ni son runtime, ni le transport utilisé pour l'atteindre.

## Structure physique

La structure de référence est :

```text
code_editor/
├── editors/
│   ├── vscode/
│   │   ├── package.json
│   │   ├── tsconfig.json
│   │   └── src/
│   │       ├── lifecycle.ts
│   │       ├── api.ts
│   │       └── commands/
│   │           ├── current_file.ts
│   │           ├── current_selection.ts
│   │           ├── open_file.ts
│   │           ├── apply_edit.ts
│   │           └── ...
│   └── bezot_editor/
├── extensions/
│   └── ai_assistant/
└── link_between_extensions_and_editors/
```

`code_editor/` n'est pas un crate Rust.

Il ne possède donc pas :

```text
code_editor/Cargo.toml
code_editor/src/lib.rs
code_editor/src/main.rs
```

Il ne faut pas transformer `code_editor/` en bibliothèque commune englobant les trois domaines.

Les frontières entre `editors/`, `extensions/` et `link_between_extensions_and_editors/` sont des frontières réelles de l'architecture.

## Extensions

Les extensions vivent dans :

```text
code_editor/extensions/
```

Par exemple :

```text
code_editor/extensions/ai_assistant/
```

`extensions/` contient plusieurs extensions possibles.

`ai_assistant` n'est donc pas synonyme de `extensions/`.

D'autres extensions pourront être ajoutées sans modifier l'organisation fondamentale :

```text
extensions/
├── ai_assistant/
├── autre_extension/
└── ...
```

### Responsabilité d'une extension

Une extension contient ce qui appartient réellement à cette extension :

- son interface utilisateur ;
- sa logique fonctionnelle ;
- son état ;
- ses échanges avec le backend ;
- ses interactions avec l'utilisateur ;
- les appels aux opérations de l'éditeur dont elle a besoin.

L'interface utilisateur appartient à l'extension.

Elle n'appartient pas à VS Code, à Bezot Editor ou à un autre éditeur.

### Indépendance vis-à-vis de l'éditeur

Une extension ne sait pas dans quel éditeur elle fonctionne.

Elle ne doit pas avoir de logique du type :

```text
si VS Code
    faire ...
sinon si Bezot Editor
    faire ...
```

Elle ne doit pas appeler directement :

```text
vscode.*
```

Elle ne doit pas non plus appeler directement une API interne propre à Bezot Editor.

Une extension exprime seulement une opération fonctionnelle.

Par exemple :

```text
current_file
current_selection
open_file
apply_edit
```

Elle peut donc demander :

```text
current_file
```

sans connaître la manière dont cette information sera obtenue.

### Exemple

L'interface de `ai_assistant` peut avoir besoin du fichier actuellement ouvert afin d'envoyer son contenu ou son contexte au backend.

Elle demande alors :

```text
current_file
```

à :

```text
link_between_extensions_and_editors/
```

Elle ne demande jamais directement :

```text
vscode.window.activeTextEditor
```

et elle ne lit jamais directement l'état interne de Bezot Editor.

### Même extension dans plusieurs éditeurs

La même logique d'extension doit pouvoir fonctionner avec plusieurs éditeurs :

```text
ai_assistant
    ↓
link_between_extensions_and_editors
    ├── VS Code
    ├── Bezot Editor
    └── autre éditeur
```

Changer d'éditeur ne doit pas imposer de réécrire la logique fonctionnelle de l'extension.

## `link_between_extensions_and_editors/`

`link_between_extensions_and_editors/` est la liaison entre les extensions et les intégrations éditeur.

Ce composant ne contient pas la logique propre à VS Code.

Il ne contient pas non plus la logique métier propre à une extension.

Son rôle est de permettre à une extension d'appeler une opération générique et d'obtenir le résultat correspondant depuis l'éditeur disponible.

### Direction des appels

La direction normale est :

```text
extension
    ↓
link_between_extensions_and_editors
    ↓
API de l'intégration éditeur
```

Pour une réponse :

```text
API de l'intégration éditeur
    ↓
link_between_extensions_and_editors
    ↓
extension
```

### Opérations fonctionnelles

La couche connaît une liste d'opérations stables.

Exemples initiaux :

```text
current_file
current_selection
open_file
apply_edit
```

D'autres opérations peuvent être ajoutées lorsqu'elles sont réellement nécessaires.

Une opération représente **ce que l'appelant demande**, jamais la manière technique de le réaliser.

Par exemple :

```text
current_file
```

signifie :

> obtenir le fichier actuellement actif dans l'éditeur.

Cela ne signifie jamais :

```text
utiliser VS Code
```

ou :

```text
utiliser WebSocket
```

ou :

```text
utiliser Rust
```

### Résultats communs

Les réponses retournées aux extensions doivent être indépendantes de l'éditeur.

L'extension ne doit pas recevoir :

```text
VsCodeCurrentFile
```

dans un environnement et :

```text
BezotEditorCurrentFile
```

dans un autre.

Elle reçoit une représentation commune correspondant à :

```text
current_file
```

Les formats spécifiques des éditeurs sont convertis avant d'atteindre l'extension.

### Transport indépendant

Les opérations et leur transport sont séparés.

Une même opération peut être appelée par :

```text
CLI
stdin/stdout
WebSocket
autre mécanisme
```

Par exemple :

```text
current_file
```

reste exactement la même opération dans chacun de ces cas.

Le transport ne fait pas partie de sa définition.

### Choix du transport par l'appelant

Le code appelant peut choisir le moyen qu'il souhaite utiliser pour joindre l'autre composant.

Il peut également essayer plusieurs moyens.

Exemple :

```text
appel current_file via stdin/stdout
    ↓
échec
    ↓
appel current_file via WebSocket
    ↓
succès
```

Le fallback appartient au code qui effectue l'appel.

Il ne doit pas modifier la logique métier de `current_file`.

### Aucune hypothèse de localité

La liaison ne doit pas supposer que les deux côtés se trouvent :

- dans le même processus ;
- sur la même machine ;
- dans le même langage ;
- dans le même runtime.

Un appel peut être local ou distant.

Le fait qu'un composant soit actuellement local ne doit pas être encodé comme une contrainte fondamentale.

### Pas de bibliothèque Rust imposée

La liaison ne doit pas obliger un appelant à intégrer une bibliothèque Rust dans son propre processus.

La frontière publique doit rester utilisable depuis différents langages et runtimes.

Lorsqu'un composant de cette couche est implémenté en Rust, il est privilégié sous forme d'exécutable autonome avec un `main.rs`.

Il ne faut pas créer un `lib.rs` simplement afin que d'autres composants puissent l'appeler directement en Rust.

Le principe est :

```text
appelant
    ↓
interface exécutable / transport
    ↓
composant
```

et non :

```text
appelant
    ↓
link obligatoire vers une lib Rust
```

Cela permet à un appelant écrit en :

```text
Rust
TypeScript
JavaScript
Python
autre langage
```

d'utiliser le même composant.

## Editors

Les intégrations propres aux différents éditeurs vivent sous :

```text
code_editor/editors/
```

Chaque éditeur possède son propre dossier.

Par exemple :

```text
editors/
├── vscode/
└── bezot_editor/
```

Tout ce qui est imposé uniquement par un éditeur reste dans son dossier.

Une contrainte de VS Code ne doit jamais devenir une contrainte globale de `code_editor/`.

## VS Code

L'intégration VS Code vit dans :

```text
code_editor/editors/vscode/
```

Elle est écrite en TypeScript.

VS Code exécute le JavaScript produit par compilation du TypeScript.

### Pourquoi TypeScript est nécessaire ici

VS Code Desktop charge les extensions dans son Extension Host basé sur Node.js.

Le point d'entrée déclaré dans `package.json` doit donc être un module JavaScript chargeable par cet environnement.

Le code source correspondant est écrit en TypeScript puis compilé.

Cette contrainte appartient uniquement à VS Code.

Elle ne signifie pas que les extensions génériques ou les autres éditeurs doivent utiliser TypeScript ou JavaScript.

### `package.json`

`package.json` indique notamment à VS Code quel fichier JavaScript constitue le point d'entrée de l'intégration.

Le nom `extension.js` n'est pas obligatoire.

Le point d'entrée choisi est le JavaScript compilé depuis :

```text
lifecycle.ts
```

Par exemple :

```json
{
  "main": "./dist/lifecycle.js"
}
```

## `lifecycle.ts`

`lifecycle.ts` représente le cycle de vie de l'intégration VS Code.

Il expose les fonctions attendues par VS Code :

```text
activate()
deactivate()
```

Le nom `lifecycle` est volontaire.

Le fichier ne sert pas uniquement à lancer quelque chose.

Il gère le démarrage **et** l'arrêt.

### Responsabilités

`activate()` initialise les composants nécessaires pendant la durée de vie de l'extension VS Code.

Il peut notamment :

- initialiser l'intégration VS Code ;
- charger les modules nécessaires ;
- enregistrer ce qui doit l'être auprès de VS Code ;
- démarrer le LSP ;
- démarrer ou initialiser `api.ts` ;
- ouvrir les ressources nécessaires à la communication avec l'extérieur.

`deactivate()` réalise l'opération inverse.

Il doit notamment :

- arrêter le LSP ;
- arrêter les services démarrés par `activate()` ;
- fermer les transports ouverts ;
- libérer les ressources ;
- terminer proprement l'intégration.

`lifecycle.ts` ne contient pas les implémentations des opérations comme `current_file`.

## Runtime VS Code

Les fichiers TypeScript ou JavaScript utilisés par l'intégration VS Code doivent être chargés dans l'arbre d'exécution démarré depuis `lifecycle.ts`.

Ils s'exécutent alors dans le même Extension Host.

C'est ce contexte d'exécution qui leur permet d'utiliser :

```ts
import * as vscode from 'vscode';
```

Le nom :

```text
vscode
```

dans cet import ne correspond pas au dossier :

```text
editors/vscode/
```

Il correspond au module fourni par l'Extension Host de VS Code.

Un fichier présent sur le disque n'obtient pas automatiquement accès à ce module.

Il doit être exécuté dans le runtime de l'extension VS Code.

## `commands/`

Les opérations internes spécifiques à VS Code vivent dans :

```text
code_editor/editors/vscode/src/commands/
```

Par exemple :

```text
commands/
├── current_file.ts
├── current_selection.ts
├── open_file.ts
└── apply_edit.ts
```

Chaque opération doit rester isolée dans son rôle.

### Exemple `current_file`

`current_file.ts` peut utiliser l'API réelle de VS Code :

```text
vscode.window.activeTextEditor
```

Il traduit ensuite cette information vers la représentation attendue par notre API externe.

### Couplage assumé

Les fichiers de `commands/` sont volontairement couplés à VS Code.

Ils peuvent utiliser :

```text
vscode.*
```

Ce couplage est nécessaire.

Il n'est pas considéré comme un défaut tant qu'il reste limité à :

```text
code_editor/editors/vscode/
```

Les autres domaines ne doivent pas importer ces fichiers directement.

## `api.ts`

`api.ts` est la seule frontière externe de l'intégration VS Code.

Tout ce qui veut obtenir une information ou déclencher une opération dans VS Code depuis l'extérieur du runtime VS Code passe par `api.ts`.

La relation est :

```text
extérieur
    ↓
api.ts
    ↓
commands/
    ↓
vscode.*
```

Pour une réponse :

```text
vscode.*
    ↓
commands/
    ↓
api.ts
    ↓
extérieur
```

### Exemple complet

Pour `current_file` :

```text
link_between_extensions_and_editors
    ↓
api.ts
    ↓
commands/current_file.ts
    ↓
vscode.window.activeTextEditor
    ↓
commands/current_file.ts
    ↓
api.ts
    ↓
link_between_extensions_and_editors
```

### Ce qui ne sort pas de VS Code

Le reste du système ne doit jamais appeler directement :

```text
lifecycle.ts
commands/
vscode.*
```

Ces éléments appartiennent au fonctionnement interne de l'intégration VS Code.

Seul :

```text
api.ts
```

est destiné à communiquer avec l'extérieur.

### Transport de `api.ts`

`api.ts` rend ses opérations accessibles par les transports réellement supportés par l'intégration.

Le transport ne fait pas partie des commandes elles-mêmes.

Il ne faut donc pas écrire une opération différente pour :

```text
current_file via WebSocket
```

et :

```text
current_file via stdin/stdout
```

Il existe une seule opération :

```text
current_file
```

et différents moyens éventuels de l'appeler.

### Couplage imposé par VS Code

L'intégration VS Code présente volontairement un couplage interne :

```text
VS Code
    ↓
lifecycle.ts
    ↓
commands/
    ↓
api.ts
```

Les trois parties doivent vivre dans l'environnement de l'extension VS Code afin que les opérations puissent utiliser `vscode.*`.

Ce couplage ne peut pas être supprimé sans perdre l'accès à l'API VS Code.

La règle consiste donc à **l'enfermer**, pas à essayer de le faire disparaître.

Sa limite est :

```text
code_editor/editors/vscode/
```

## Pipeline VS Code

### Activation

```text
VS Code
    ↓
package.json
    ↓
dist/lifecycle.js
    ↓
activate()
    ↓
initialisation commands/
    ↓
initialisation api.ts
    ↓
démarrage LSP et ressources nécessaires
```

### Appel `current_file`

```text
interface utilisateur de l'extension
    ↓
link_between_extensions_and_editors
    ↓
api.ts
    ↓
commands/current_file.ts
    ↓
vscode.window.activeTextEditor
```

Puis :

```text
vscode.window.activeTextEditor
    ↓
commands/current_file.ts
    ↓
api.ts
    ↓
link_between_extensions_and_editors
    ↓
interface utilisateur de l'extension
```

### Désactivation

```text
VS Code
    ↓
deactivate()
    ↓
arrêt LSP
    ↓
arrêt API / transports
    ↓
libération des ressources
```

## Bezot Editor

L'intégration du futur éditeur natif vit dans :

```text
code_editor/editors/bezot_editor/
```

Bezot Editor n'a pas à reproduire l'architecture interne imposée par VS Code.

Il n'a notamment aucune obligation d'utiliser :

- Node.js ;
- TypeScript ;
- JavaScript ;
- l'Extension Host ;
- `lifecycle.ts` ;
- la même manière d'exposer son API interne.

### Même opération, autre implémentation

Pour une extension, l'opération reste :

```text
current_file
```

Avec VS Code :

```text
current_file
    ↓
api.ts
    ↓
commands/current_file.ts
    ↓
vscode.*
```

Avec Bezot Editor :

```text
current_file
    ↓
intégration Bezot Editor
    ↓
état interne natif de l'éditeur
```

Si Bezot Editor est écrit entièrement en Rust, l'intégration peut obtenir cette information directement depuis son état Rust.

Cela ne change rien à l'opération vue depuis l'extension.

## Pipeline global

Le pipeline général est :

```text
Utilisateur
    ↓
interface utilisateur de l'extension
    ↓
extension
    ↓
link_between_extensions_and_editors
    ↓
API de l'intégration éditeur
    ↓
éditeur
```

La réponse revient par le chemin inverse :

```text
éditeur
    ↓
API de l'intégration éditeur
    ↓
link_between_extensions_and_editors
    ↓
extension
    ↓
interface utilisateur
```

## Backend

Le backend reste séparé de l'intégration éditeur.

Une extension peut communiquer avec le backend pour :

- envoyer une demande ;
- envoyer du contexte ;
- recevoir une réponse ;
- utiliser les fonctions IA de la plateforme.

L'accès à l'éditeur passe en parallèle par :

```text
link_between_extensions_and_editors/
```

Le backend ne doit pas connaître :

```text
vscode.*
```

ni la structure interne d'un éditeur particulier.

Le flux peut donc contenir deux relations différentes :

```text
extension
    ├── backend
    └── link_between_extensions_and_editors
            ↓
          editor
```

Ces deux responsabilités ne doivent pas être fusionnées.

## Rust

Rust reste le langage privilégié pour les composants qui peuvent être écrits en Rust.

Cela ne signifie pas que les contraintes propres à VS Code doivent être contournées artificiellement.

L'intégration VS Code reste en TypeScript là où VS Code impose son runtime.

### Exécutables

Lorsqu'un composant Rust doit être exposé comme composant autonome, il utilise un exécutable avec :

```text
src/main.rs
```

La frontière publique privilégiée est l'exécutable et ses moyens d'appel.

### Pas de `lib.rs` comme frontière obligatoire

Il ne faut pas imposer une bibliothèque Rust simplement parce que plusieurs composants doivent communiquer.

Une telle bibliothèque obligerait les consommateurs à intégrer directement du Rust ou une ABI associée.

Cela va à l'encontre de l'objectif d'indépendance des langages et runtimes.

### `main.rs` ne signifie pas CLI

Le fait qu'un composant possède un `main.rs` signifie qu'il est exécutable.

Cela ne signifie pas que son interface publique doit être uniquement une CLI.

Le même exécutable peut éventuellement accepter différents mécanismes :

```text
CLI
stdin/stdout
WebSocket
autre
```

selon ce qui est réellement implémenté.

## Opérations initiales

Les premières opérations communes identifiées sont :

```text
current_file
current_selection
open_file
apply_edit
```

Elles constituent le début du vocabulaire commun entre extensions et éditeurs.

### `current_file`

Retourne le fichier actuellement actif dans l'éditeur.

### `current_selection`

Retourne la sélection courante de l'utilisateur.

### `open_file`

Demande à l'éditeur d'ouvrir un fichier.

### `apply_edit`

Demande à l'éditeur d'appliquer une modification.

Ces opérations ne sont jamais liées à un transport particulier.

## Règles obligatoires

### Extensions

1. Une extension ne connaît pas l'éditeur qui l'héberge.
2. L'interface utilisateur appartient à l'extension.
3. Une extension ne dépend jamais directement de `vscode.*`.
4. Une extension utilise les opérations communes pour accéder à l'éditeur.
5. Plusieurs extensions peuvent réutiliser les mêmes intégrations éditeur.

### Liaison

1. `link_between_extensions_and_editors/` est la seule voie normale entre une extension générique et une intégration éditeur.
2. Une opération décrit un besoin, pas son implémentation.
3. Les résultats sont normalisés avant d'atteindre l'extension.
4. Le transport est séparé des opérations.
5. L'appelant peut choisir ou changer de transport.
6. Un fallback entre transports est autorisé.
7. Aucun transport n'est globalement obligatoire.
8. Aucune hypothèse de localité n'est autorisée.
9. La frontière publique ne doit pas imposer une bibliothèque Rust.

### Editors

1. Tout ce qui est spécifique à un éditeur reste dans son dossier.
2. Un éditeur peut implémenter les mêmes opérations d'une manière totalement différente d'un autre.
3. Les contraintes d'un éditeur ne deviennent jamais des contraintes globales.
4. Un nouvel éditeur doit pouvoir être ajouté sans réécrire les extensions.

### VS Code

1. L'intégration est écrite en TypeScript.
2. VS Code charge le JavaScript compilé.
3. `lifecycle.ts` gère `activate()` et `deactivate()`.
4. `lifecycle.ts` démarre et arrête notamment le LSP et les ressources nécessaires.
5. `commands/` contient les opérations internes utilisant `vscode.*`.
6. `api.ts` est la seule frontière externe de l'intégration VS Code.
7. Le reste de la plateforme ne doit jamais appeler directement `commands/`.
8. Le reste de la plateforme ne doit jamais dépendre directement de `vscode.*`.
9. `lifecycle.ts`, `commands/` et `api.ts` vivent dans le runtime VS Code.
10. Ce couplage interne est accepté et reste confiné à `editors/vscode/`.

### Rust

1. `code_editor/` n'est pas un crate Rust.
2. Ne pas créer `code_editor/src/lib.rs`.
3. Ne pas créer une bibliothèque commune uniquement pour faire communiquer les composants.
4. Les composants Rust autonomes utilisent un `main.rs`.
5. Un exécutable Rust peut exposer plusieurs transports.
6. L'appelant ne doit pas être obligé d'être écrit en Rust.

## Référence finale

La structure conceptuelle est :

```text
extensions
    ↓
link_between_extensions_and_editors
    ↓
editors
```

La structure VS Code est :

```text
VS Code
    ↓
lifecycle.ts
    ↓
commands/
    ↓
api.ts
    ↑
link_between_extensions_and_editors
    ↑
extensions
```

La structure avec Bezot Editor conserve la même vue depuis l'extension :

```text
extension
    ↓
link_between_extensions_and_editors
    ↓
Bezot Editor
```

mais l'implémentation interne peut être entièrement native.

La règle centrale est :

> Une extension demande ce qu'elle veut faire.
> La liaison détermine comment joindre l'éditeur.
> L'intégration de l'éditeur sait comment réaliser l'opération.
> L'extension ne connaît jamais ces détails.

## Interface utilisateur de `ai_assistant`

L'interface utilisateur de `ai_assistant` est développée en Rust avec Slint 1.18.1.

Ce choix concerne uniquement l'interface de code_editor/extensions/ai_assistant/.

Il ne définit pas la technologie des éditeurs, du backend, de link_between_extensions_and_editors/, des autres extensions ou d'une future application hôte.

### Native-first sans imposer le web

Slint est utilisé avec une approche native-first.

L'interface doit pouvoir fonctionner nativement sans dépendre obligatoirement d'un navigateur, du DOM, d'une WebView ou d'un runtime web.

Le web reste néanmoins une cible possible.

Un environnement qui a besoin d'exécuter ou d'intégrer l'interface via une cible web doit pouvoir le faire sans imposer cette solution aux environnements capables d'utiliser l'interface native.

La possibilité d'utiliser le web ne doit donc jamais devenir une obligation architecturale.

### Séparation avec le reste de la plateforme

Slint est un détail d'implémentation de l'interface utilisateur de `ai_assistant`.

La logique fonctionnelle de l'extension ne doit pas dépendre inutilement de Slint.

Les frontières existantes restent inchangées entre `ai_assistant`, le backend, `link_between_extensions_and_editors` et les éditeurs.

Slint ne doit pas devenir un protocole de communication entre ces composants.

### Validation obligatoire de Slint

Le choix de Slint doit être validé pendant le développement de l'interface réelle de `ai_assistant`.

Cette validation est obligatoire car les performances et la consommation mémoire de Slint sur une interface contenant beaucoup de texte, de Markdown et de code doivent être vérifiées dans notre usage réel.

Une fenêtre minimale, un exemple ou un Hello World ne constitue pas une validation suffisante.

La validation doit utiliser une charge représentative d'un assistant de programmation comprenant notamment :

- une conversation longue ;
- beaucoup de texte ;
- de nombreux blocs de code ;
- du Markdown ;
- un historique important ;
- le défilement continu de cet historique ;
- des réponses reçues progressivement par streaming.

### Mémoire à mesurer

Il faut mesurer séparément :

- la RAM système utilisée pendant une compilation debug ;
- la RAM système utilisée pendant une compilation release ;
- la RAM système utilisée par l'interface au repos ;
- la RAM système utilisée avec une conversation importante ;
- la RAM système utilisée pendant le défilement ;
- la RAM système utilisée pendant le streaming d'une réponse ;
- la mémoire GPU lorsque celle-ci peut être mesurée.

La RAM système et la VRAM ne doivent jamais être confondues.

Les valeurs réellement observées doivent être rapportées.

Il ne faut pas inventer de seuil arbitraire pour déclarer Slint acceptable ou non.

### Performances à vérifier

Il faut également vérifier :

- la fluidité du défilement ;
- la réactivité avec beaucoup de texte ;
- la réactivité avec de nombreux blocs de code ;
- le coût du Markdown ;
- les temps de réponse pendant le streaming ;
- les éventuels freezes ;
- les ralentissements sur les gros historiques ;
- la croissance de la consommation mémoire pendant une utilisation prolongée ;
- le coût réel de compilation de l'interface.

Si l'utilisation représentative révèle une consommation mémoire disproportionnée, une croissance anormale de la mémoire, des ralentissements importants, des freezes ou un coût de compilation problématique, cela doit être signalé avant de poursuivre davantage le développement de l'interface sur cette base.

Le choix de Slint est donc le choix actuel pour implémenter `ai_assistant`, mais sa validation repose sur des mesures réelles effectuées sur notre interface et non sur des exemples théoriques.
