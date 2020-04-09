import os
import dash
import dotenv
import pandas as pd
import dash_core_components as dcc
import dash_html_components as html
import plotly.graph_objects as go
from dash.dependencies import Input, Output

from . import queries

dotenv.load_dotenv()


database_url = os.environ["CRATESTATS_DATABASE_URL"]

# Data setup
categories_df = pd.read_sql(queries.downloads_per_category(), database_url,)

# App setup


app = dash.Dash(__name__)

app.layout = html.Div(
    [
        html.H1("cratestats.io"),
        html.Div(
            [
                html.H2("Crates by category"),
                dcc.Slider(
                    id="num-categories-slider",
                    min=1,
                    max=len(categories_df),
                    value=len(categories_df),
                ),
                dcc.Graph(
                    id="crate-categories",
                    figure={
                        "data": [
                            {
                                "x": categories_df["category"],
                                "y": categories_df["crate_count"],
                                "type": "bar",
                            },
                        ],
                        "layout": {"title": "Crates by category",},
                    },
                ),
                dcc.Graph(
                    id="crate-categories-pie",
                    figure=go.Figure(
                        go.Pie(
                            labels=categories_df["category"],
                            values=categories_df["crate_count"],
                        )
                    ),
                ),
            ]
        ),
    ]
)


@app.callback(
    [Output("crate-categories", "figure"), Output("crate-categories-pie", "figure")],
    [Input("num-categories-slider", "value")],
)
def update_num_categories(num_categories):
    filtered_df = categories_df.head(num_categories)
    bar_result = {
        "data": [
            {
                "x": filtered_df["category"],
                "y": filtered_df["crate_count"],
                "type": "bar",
            },
        ],
        "layout": {"title": "Crates by category",},
    }
    pie_result = go.Figure(
        go.Pie(labels=filtered_df["category"], values=filtered_df["crate_count"],)
    )

    return bar_result, pie_result