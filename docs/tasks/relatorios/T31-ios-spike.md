# Relatório T31 — iOS/iPadOS

Data: 29/09/2026  
Status: Concluída como spike

Decisão: adiar cliente iOS/iPadOS. O repositório não tem cliente Swift nem host Mac/iPad para testar VideoToolbox, Network framework, áudio, Pencil, lifecycle, Stage Manager, rede local, USB ou distribuição.

Uma continuação deve separar LAN de USB, preservar T02/T11, negociar capacidades por aparelho e tratar background/lock/orientação como limites próprios. ADB/AOA não serão presumidos como transporte Apple e nenhuma conta, entitlement ou submissão será criada sem autorização.
